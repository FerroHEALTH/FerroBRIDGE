// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one [`Handler`] the inbound workers run, where every inbound step is
//! written: the source's patient resolved, minimised, fed to the identity
//! source, given its ITI-83 pseudonym and exactly one EHR.
//!
//! The bridge does no matching, linking or merging of its own; the identity
//! source does. An inbound event never deletes an EHR: a remove only tells
//! the identity source the patient is no longer active. No specification
//! governs the order of the steps: our own design over `PIXm` 3.1.0 and the
//! ITS-REST 1.1.0 EHR API.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use fhir_types::codec::Value;
use ihe_atna::user::OnBehalfOf;
use ihe_iti::pixm::identifier::SourceIdentifier;
use secrecy::SecretString;

use super::event::Change;
use super::retry::Failure;
use super::store::Claimed;
use super::worker::{Attempt, Failed, Handler};
use crate::cdr::CdrClient;
use crate::facade::ehr::{self, EhrError, Policy};
use crate::facade::identity::store::Store;
use crate::identity_source::IdentitySource;
use crate::sources::Source;
use crate::sources::document::Tenant;

/// The `Patient` elements the identity source is fed, and nothing else
/// (GDPR Art 5(1)(c)).
const KEPT: [&str; 6] = [
    "identifier",
    "name",
    "birthDate",
    "gender",
    "deceasedBoolean",
    "deceasedDateTime",
];

/// The future of one step that answers a value.
pub type Step<'a, T> = Pin<Box<dyn Future<Output = Result<T, Failed>> + Send + 'a>>;

/// How a source's entity is read as an R4 `Patient`.
pub trait Resolve: std::fmt::Debug + Send + Sync {
    /// Returns the `Patient` the source holds for `entity` of `tenant`.
    fn patient<'a>(
        &'a self,
        source: &'a Source,
        tenant: &'a Tenant,
        entity: &'a str,
    ) -> Step<'a, Value>;
}

/// The resolution of a source that declares none, which parks every item.
#[derive(Debug, Clone, Copy, Default)]
pub struct Undeclared;

impl Resolve for Undeclared {
    fn patient<'a>(&'a self, _: &'a Source, _: &'a Tenant, _: &'a str) -> Step<'a, Value> {
        Box::pin(std::future::ready(Err(Failed {
            failure: Failure::Refusal,
            reason: "resolution-undeclared",
        })))
    }
}

/// How a patient reaches the identity source (ITI-104, `PIXm` 3.1.0
/// §2:3.104).
pub trait Feed: std::fmt::Debug + Send + Sync {
    /// Creates or updates the patient `patient` of `tenant`.
    fn upsert<'a>(
        &'a self,
        tenant: &'a Tenant,
        entity: &'a str,
        patient: &'a Value,
    ) -> Step<'a, ()>;

    /// Marks the patient `entity` of `tenant` inactive (`Patient.active =
    /// false`).
    fn deactivate<'a>(&'a self, tenant: &'a Tenant, entity: &'a str) -> Step<'a, ()>;
}

/// The feed while no published client speaks ITI-104, which parks every
/// item.
// TODO(#2048): feed through the ihe-iti Patient Identity Source client of FerroHEALTH/FerroPIX#799.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unavailable;

impl Unavailable {
    /// The failure every feed answers.
    const FAILED: Failed = Failed {
        failure: Failure::Refusal,
        reason: "feed-client-unavailable",
    };
}

impl Feed for Unavailable {
    fn upsert<'a>(&'a self, _: &'a Tenant, _: &'a str, _: &'a Value) -> Step<'a, ()> {
        Box::pin(std::future::ready(Err(Self::FAILED)))
    }

    fn deactivate<'a>(&'a self, _: &'a Tenant, _: &'a str) -> Step<'a, ()> {
        Box::pin(std::future::ready(Err(Self::FAILED)))
    }
}

/// The handler: what it resolves through, feeds, asks and writes into.
#[derive(Debug)]
pub struct Apply {
    /// How an entity is read.
    resolve: Arc<dyn Resolve>,
    /// How a patient is fed.
    feed: Arc<dyn Feed>,
    /// The identity source asked for the pseudonym.
    identity: Arc<IdentitySource>,
    /// The CDR the EHR is ensured in.
    client: CdrClient,
    /// The identity map.
    store: Arc<dyn Store>,
}

impl Apply {
    /// Returns the handler over its seams, the identity source, the CDR and
    /// the identity map.
    #[must_use]
    pub fn new(
        (resolve, feed): (Arc<dyn Resolve>, Arc<dyn Feed>),
        identity: Arc<IdentitySource>,
        client: CdrClient,
        store: Arc<dyn Store>,
    ) -> Self {
        Self {
            resolve,
            feed,
            identity,
            client,
            store,
        }
    }

    /// Creates or updates the patient, then ensures its one EHR.
    async fn upsert(&self, source: &Source, tenant: &Tenant, entity: &str) -> Result<(), Failed> {
        let resolved = self.resolve.patient(source, tenant, entity).await?;
        let patient = minimised(&resolved, tenant, entity);
        self.feed.upsert(tenant, entity, &patient).await?;
        let local = SourceIdentifier::new(
            tenant.identifier_system.as_str(),
            SecretString::from(entity),
        )
        .map_err(|_invalid| refused("local-identifier-invalid"))?;
        let person = self
            .identity
            .pseudonym(&local, &OnBehalfOf::System)
            .await
            .map_err(|error| Failed {
                failure: if error.transient() {
                    Failure::Transient
                } else {
                    Failure::Refusal
                },
                reason: "pseudonym-refused",
            })?;
        ehr::resolve(
            &self.client,
            self.store.as_ref(),
            &person,
            Policy::CreateOnFirstWrite,
        )
        .await
        .map(drop)
        .map_err(|error| ehr_failed(&error))
    }
}

impl Handler for Apply {
    fn apply<'a>(&'a self, source: &'a Source, item: &'a Claimed) -> Attempt<'a> {
        Box::pin(async move {
            let tenant = source
                .parsed
                .tenants()
                .iter()
                .find(|tenant| tenant.name == item.key.tenant && tenant.enabled)
                .ok_or_else(|| refused("tenant-unknown"))?;
            match item.change {
                Change::Upsert => self.upsert(source, tenant, &item.key.entity).await,
                Change::Remove => self.feed.deactivate(tenant, &item.key.entity).await,
            }
        })
    }
}

/// Returns the failure for good of `reason`.
const fn refused(reason: &'static str) -> Failed {
    Failed {
        failure: Failure::Refusal,
        reason,
    }
}

/// Returns how a failed EHR resolution fails the item.
fn ehr_failed(error: &EhrError) -> Failed {
    let failure = match error {
        EhrError::Client { source } => source
            .upstream_status()
            .map_or(Failure::Transient, Failure::of),
        _ => Failure::Refusal,
    };
    match failure {
        Failure::Transient => Failed {
            failure,
            reason: "ehr-unavailable",
        },
        Failure::Refusal => refused("ehr-refused"),
    }
}

/// Returns `patient` holding only the [`KEPT`] elements, with the tenant's
/// local identifier for `entity` among its identifiers.
#[must_use]
pub fn minimised(patient: &Value, tenant: &Tenant, entity: &str) -> Value {
    let mut kept = fhir_types::codec::Object::new();
    kept.insert(
        String::from("resourceType"),
        Value::String(String::from("Patient")),
    );
    if let Value::Object(object) = patient {
        for name in KEPT {
            if let Some(value) = object.get(name) {
                kept.insert(String::from(name), value.clone());
            }
        }
    }
    let local = |identifier: &Value| {
        identifier.get("system").and_then(Value::as_str) == Some(tenant.identifier_system.as_str())
            && identifier.get("value").and_then(Value::as_str) == Some(entity)
    };
    let mut identifiers = match kept.remove("identifier") {
        Some(Value::Array(identifiers)) => identifiers,
        _ => Vec::new(),
    };
    if !identifiers.iter().any(local) {
        let mut identifier = fhir_types::codec::Object::new();
        identifier.insert(
            String::from("system"),
            Value::String(tenant.identifier_system.clone()),
        );
        identifier.insert(String::from("value"), Value::String(String::from(entity)));
        identifiers.insert(0, Value::Object(identifier));
    }
    kept.insert(String::from("identifier"), Value::Array(identifiers));
    Value::Object(kept)
}
