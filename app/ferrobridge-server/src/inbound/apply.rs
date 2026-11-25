// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one [`Handler`] the inbound workers run, where every inbound step is
//! written.
//!
//! The source's patient is resolved, minimised, fed to the identity source
//! over ITI-104, given its ITI-83 pseudonym and exactly one EHR.
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
use ihe_atna::balp::{NetworkAddress, Reserved};
use ihe_atna::user::OnBehalfOf;
use ihe_iti::pixm::identifier::SourceIdentifier;
use ihe_iti::pixm::source::error::SourceError as FeedError;
use secrecy::SecretString;

use super::event::Change;
use super::store::{Claimed, InboxStore};
use super::worker::{Attempt, Failed, Handler};
use crate::audit::AccessLog;
use crate::cdr::CdrClient;
use crate::facade::ehr::{self, EhrError, Policy};
use crate::facade::identity::store::Store;
use crate::identity_source::IdentitySource;
use crate::retry::Failure;
use crate::sources::Source;
use crate::sources::document::{Document, Tenant};

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
            failure: Failure::Poisoned,
            reason: "resolution-undeclared",
        })))
    }
}

/// The handler: what it resolves through, feeds, asks and writes into.
#[derive(Debug)]
pub struct Apply {
    /// How an entity is read.
    resolve: Arc<dyn Resolve>,
    /// The identity source fed and asked for the pseudonym.
    identity: Arc<IdentitySource>,
    /// The CDR the EHR is ensured in.
    client: CdrClient,
    /// The identity map.
    store: Arc<dyn Store>,
    /// The access-record lane each step is recorded in, under `[audit]`.
    access_log: Option<Arc<AccessLog>>,
    /// The inbox whose sealed event bodies carry an MLLP source's patient.
    inbox: Option<InboxStore>,
}

impl Apply {
    /// Returns the handler over `resolve`, the identity source, the CDR and
    /// the identity map, recording nothing.
    #[must_use]
    pub fn new(
        resolve: Arc<dyn Resolve>,
        identity: Arc<IdentitySource>,
        client: CdrClient,
        store: Arc<dyn Store>,
    ) -> Self {
        Self {
            resolve,
            identity,
            client,
            store,
            access_log: None,
            inbox: None,
        }
    }

    /// Returns this handler reading an MLLP source's patient from the
    /// sealed body of the event it applies in `inbox`.
    #[must_use]
    pub fn with_inbox(self, inbox: Option<InboxStore>) -> Self {
        Self { inbox, ..self }
    }

    /// Returns this handler recording each feed and each EHR it creates in
    /// `log`.
    #[must_use]
    pub fn with_access_log(self, log: Option<Arc<AccessLog>>) -> Self {
        Self {
            access_log: log,
            ..self
        }
    }

    /// Reads the source's patient through its resolution and applies it.
    async fn upsert(&self, source: &Source, tenant: &Tenant, entity: &str) -> Result<(), Failed> {
        let resolved = self.resolve.patient(source, tenant, entity).await?;
        self.upsert_resolved(source, tenant, entity, &resolved)
            .await
    }

    /// Applies the patient an MLLP source's event carries: the tenant is the
    /// identifier system the face settled the patient's identifier in.
    async fn carried(&self, source: &Source, item: &Claimed) -> Result<(), Failed> {
        let inbox = self
            .inbox
            .as_ref()
            .ok_or_else(|| refused("inbox-unavailable"))?;
        let body = inbox
            .body(&item.key, item.generation)
            .await
            .map_err(|_unread| Failed {
                failure: Failure::Transient,
                reason: "event-body-unreadable",
            })?
            .ok_or_else(|| refused("event-body-absent"))?;
        let resolved: Value = serde_json::from_slice(body.as_bytes())
            .map_err(|_unread| refused("patient-unreadable"))?;
        let tenant = Tenant {
            name: item.key.tenant.clone(),
            identifier_system: item.key.tenant.clone(),
            credentials: None,
            enabled: true,
        };
        self.upsert_resolved(source, &tenant, &item.key.entity, &resolved)
            .await
    }

    /// Creates or updates the `resolved` patient, or forwards the source's
    /// merge, then ensures the patient's one EHR.
    async fn upsert_resolved(
        &self,
        source: &Source,
        tenant: &Tenant,
        entity: &str,
        resolved: &Value,
    ) -> Result<(), Failed> {
        let local = local_of(tenant, entity)?;
        let patient = typed(&minimised(resolved, tenant, entity))?;
        let survivor = survivor_of(resolved, tenant)?;
        let place = self.reserve().await?;
        match survivor.as_ref() {
            // NOTE: PIXm 3.1.0 §2:3.104.4.2: the source's own merge goes to the identity
            // source as Resolve Duplicate; the survivor's own upsert ensures its EHR.
            Some(survivor) => self
                .identity
                .merge((&local, survivor), &patient)
                .await
                .map_err(|error| fed_failed(&error))?,
            None => self
                .identity
                .feed(&local, &patient)
                .await
                .map_err(|error| fed_failed(&error))?,
        }
        self.imported(place, source, tenant, entity).await?;
        if survivor.is_some() {
            return Ok(());
        }
        self.ensure_ehr(&local).await
    }

    /// Tells the identity source the patient is no longer active
    /// (`Patient.active = false`); an EHR is never deleted.
    async fn deactivate(
        &self,
        source: &Source,
        tenant: &Tenant,
        entity: &str,
    ) -> Result<(), Failed> {
        let local = local_of(tenant, entity)?;
        let patient = fhir_types::r4::patient::Patient {
            identifier: vec![fhir_types::r4::identifier::Identifier {
                system: Some(tenant.identifier_system.as_str().into()),
                value: Some(entity.into()),
                ..fhir_types::r4::identifier::Identifier::default()
            }],
            active: Some(false.into()),
            ..fhir_types::r4::patient::Patient::default()
        };
        let place = self.reserve().await?;
        self.identity
            .feed(&local, &patient)
            .await
            .map_err(|error| fed_failed(&error))?;
        self.imported(place, source, tenant, entity).await
    }

    /// Asks the pseudonym of `local` and ensures its one EHR, recording a
    /// create.
    async fn ensure_ehr(&self, local: &SourceIdentifier) -> Result<(), Failed> {
        let person = self
            .identity
            .pseudonym(local, &OnBehalfOf::System)
            .await
            .map_err(|error| Failed {
                failure: if error.transient() {
                    Failure::Transient
                } else {
                    Failure::Poisoned
                },
                reason: "pseudonym-refused",
            })?;
        let place = self.reserve().await?;
        let ensured = ehr::resolve(
            &self.client,
            self.store.as_ref(),
            &person,
            Policy::CreateOnFirstWrite,
        )
        .await
        .map_err(|error| ehr_failed(&error))?;
        let Some(log) = self.access_log.as_ref().filter(|_| ensured.created) else {
            return Ok(());
        };
        let exchange = crate::audit::inbound::ehr_created(
            NetworkAddress::uri(self.client.base_url()),
            crate::audit::inbound::Patient {
                system: person.namespace(),
                value: person.id(),
            },
            ensured.id.as_str(),
        );
        record(log, place, exchange).await
    }

    /// Takes the place of one record before a step, so a step whose record
    /// has no place is not taken.
    async fn reserve(&self) -> Result<Option<Box<dyn Reserved>>, Failed> {
        match self.access_log.as_ref() {
            Some(log) => log.recorder().reserve().await.map_err(|_full| unrecorded()),
            None => Ok(None),
        }
    }

    /// Records the import of the patient `entity` of `tenant` from `source`.
    async fn imported(
        &self,
        place: Option<Box<dyn Reserved>>,
        source: &Source,
        tenant: &Tenant,
        entity: &str,
    ) -> Result<(), Failed> {
        let Some(log) = self.access_log.as_ref() else {
            return Ok(());
        };
        let exchange = crate::audit::inbound::imported(
            &source.name,
            log.endpoint().clone(),
            crate::audit::inbound::Patient {
                system: &tenant.identifier_system,
                value: entity,
            },
        );
        record(log, place, exchange).await
    }
}

impl Handler for Apply {
    fn apply<'a>(&'a self, source: &'a Source, item: &'a Claimed) -> Attempt<'a> {
        Box::pin(async move {
            if matches!(source.parsed, Document::Mllp(_)) {
                return self.carried(source, item).await;
            }
            let tenant = source
                .parsed
                .tenants()
                .iter()
                .find(|tenant| tenant.name == item.key.tenant && tenant.enabled)
                .ok_or_else(|| refused("tenant-unknown"))?;
            match item.change {
                Change::Upsert => self.upsert(source, tenant, &item.key.entity).await,
                Change::Remove => self.deactivate(source, tenant, &item.key.entity).await,
            }
        })
    }
}

/// Returns the failure for good of `reason`.
const fn refused(reason: &'static str) -> Failed {
    Failed {
        failure: Failure::Poisoned,
        reason,
    }
}

/// Records `exchange` in `place` or, with no place, through `log`'s recorder.
async fn record(
    log: &AccessLog,
    place: Option<Box<dyn Reserved>>,
    exchange: ihe_atna::balp::Exchange,
) -> Result<(), Failed> {
    match place {
        Some(place) => place.record(exchange).await,
        None => log.recorder().record(exchange).await,
    }
    .map_err(|_unrecorded| unrecorded())
}

/// Returns the failure of a step whose record the outbox could not take,
/// which runs again.
const fn unrecorded() -> Failed {
    Failed {
        failure: Failure::Transient,
        reason: "audit-unrecorded",
    }
}

/// Returns the identifier `entity` has in `tenant`'s domain.
fn local_of(tenant: &Tenant, entity: &str) -> Result<SourceIdentifier, Failed> {
    SourceIdentifier::new(
        tenant.identifier_system.as_str(),
        SecretString::from(entity),
    )
    .map_err(|_invalid| refused("local-identifier-invalid"))
}

/// Returns `patient` as an R4 `Patient`.
fn typed(patient: &Value) -> Result<fhir_types::r4::patient::Patient, Failed> {
    use fhir_types::codec::Json as _;
    let Value::Object(object) = patient else {
        return Err(refused("patient-unreadable"));
    };
    fhir_types::r4::patient::Patient::from_json(
        object,
        &mut fhir_types::codec::Path::root("Patient"),
    )
    .map_err(|_unread| refused("patient-unreadable"))
}

/// Returns the identifier of the patient a source merged `patient` into, by
/// its `replaced-by` link, when it carries one (R4 `link-type`).
fn survivor_of(patient: &Value, tenant: &Tenant) -> Result<Option<SourceIdentifier>, Failed> {
    let Some(Value::Array(links)) = patient.get("link") else {
        return Ok(None);
    };
    let Some(link) = links
        .iter()
        .find(|link| link.get("type").and_then(Value::as_str) == Some("replaced-by"))
    else {
        return Ok(None);
    };
    let identifier = link.get("other").and_then(|other| other.get("identifier"));
    let system = identifier
        .and_then(|identifier| identifier.get("system"))
        .and_then(Value::as_str);
    let value = identifier
        .and_then(|identifier| identifier.get("value"))
        .and_then(Value::as_str);
    match (system, value) {
        (Some(system), Some(value)) if system == tenant.identifier_system => {
            SourceIdentifier::new(system, SecretString::from(value))
                .map(Some)
                .map_err(|_invalid| refused("merge-survivor-invalid"))
        }
        _ => Err(refused("merge-survivor-unidentified")),
    }
}

/// Returns how a failed ITI-104 message fails the item: `5xx`, `429`, `423`,
/// no answer or no credential run again, every other answer parks.
fn fed_failed(error: &FeedError) -> Failed {
    let failure = match error {
        FeedError::Rejected { status, .. } => super::STATUSES.failure(*status),
        FeedError::Timeout
        | FeedError::Transport(_)
        | FeedError::Unauthenticated(_)
        | FeedError::Audit(_) => Failure::Transient,
        _ => Failure::Poisoned,
    };
    match failure {
        Failure::Transient => Failed {
            failure,
            reason: "feed-unavailable",
        },
        Failure::Poisoned => refused("feed-refused"),
    }
}

/// Returns how a failed EHR resolution fails the item.
fn ehr_failed(error: &EhrError) -> Failed {
    let failure = match error {
        EhrError::Client { source } => source
            .upstream_status()
            .map_or(Failure::Transient, |status| super::STATUSES.failure(status)),
        _ => Failure::Poisoned,
    };
    match failure {
        Failure::Transient => Failed {
            failure,
            reason: "ehr-unavailable",
        },
        Failure::Poisoned => refused("ehr-refused"),
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
