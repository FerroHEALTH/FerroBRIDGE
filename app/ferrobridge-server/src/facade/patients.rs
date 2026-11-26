// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The facade's resolution of a subject to its person.
//!
//! It runs over the subject link of [`crate::identity::subject`]: the
//! `Patient` party context the programs load, the identity source's
//! pseudonym, and the `OperationOutcome` refusal of each failure.

use std::sync::Arc;

use fhir_types::codec::Value;
use fhirconnect::engine::demographics::Demographics;
use http::StatusCode;
use ihe_atna::user::OnBehalfOf;
use ihe_iti::pixm::identifier::SourceIdentifier;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;

use crate::cdr::CdrClient;
use crate::facade::ingest::Refused;
use crate::facade::outcome::{Issue, IssueType};
use crate::facade::programs::Programs;
use crate::identity::PersonId;
use crate::identity::store::Store;
use crate::identity::subject::{PATIENT, Subject, SubjectLink};
use crate::identity_source::PseudonymError;
use crate::terminology::Translator;

/// What resolving a subject to its person reaches: the identity source, the
/// identity map, and the `Patient` party context with the CDR it reads.
#[derive(Debug)]
pub struct Patients<'a> {
    /// The subject link.
    pub(crate) link: &'a SubjectLink,
    /// The identity map.
    pub(crate) store: &'a dyn Store,
    /// The programs, whose `Patient` party context decides the id form.
    pub(crate) programs: Arc<Programs>,
    /// The CDR client a party is read through.
    pub(crate) client: &'a CdrClient,
    /// What a party is rendered through.
    pub(crate) renders: (&'a Translator, &'a dyn Demographics),
    /// Whom the identity source is asked for on behalf of.
    pub(crate) on_behalf: OnBehalfOf,
}

impl Patients<'_> {
    /// Returns the person `subject` is.
    ///
    /// # Errors
    ///
    /// Returns the refusals of [`Patients::person_of_patient`] and of a
    /// failed pseudonym query.
    pub async fn person(&self, subject: Subject) -> Result<PersonId, Refused> {
        match subject {
            Subject::Identified(source) => self.pseudonym(&source).await,
            Subject::Patient(id) => self.person_of_patient(&id).await,
        }
    }

    /// Returns the person the `Patient` with logical id `id` is.
    ///
    /// Under a `Patient` party context the identity map answers, and on a
    /// miss the party is read, its identifier in the first accepted domain
    /// asked of the identity source, and the answer recorded.
    ///
    /// # Errors
    ///
    /// Returns a `422` for an id that names no person or no party the CDR
    /// holds, the refusals of the party read and of a failed pseudonym
    /// query, and a `500` when the identity map cannot be used.
    pub async fn person_of_patient(&self, id: &str) -> Result<PersonId, Refused> {
        if self.programs.party(PATIENT).is_none() {
            return self.link.person_of_pseudonym(id).map_err(|error| {
                unprocessable(IssueType::Value, ferrohealth_server::run::chain(&error))
            });
        }
        self.known_person(id).await?.ok_or_else(|| {
            unprocessable(
                IssueType::NotFound,
                format!("{PATIENT}/{id} names no {PATIENT} party the CDR holds"),
            )
        })
    }

    /// Returns the person the `Patient` with logical id `id` is, as
    /// [`Patients::person_of_patient`] resolves it, `None` for an id that
    /// names no person or no party the CDR holds.
    ///
    /// # Errors
    ///
    /// Returns the refusals of the party read and of a failed pseudonym
    /// query, a `422` for a party with no identifier in an accepted domain,
    /// and a `500` when the identity map cannot be used.
    pub async fn known_person(&self, id: &str) -> Result<Option<PersonId>, Refused> {
        let Some(loaded) = self.programs.party(PATIENT) else {
            // NOTE: an id that cannot be a pseudonym legitimately names no person.
            return Ok(self.link.person_of_pseudonym(id).ok());
        };
        if let Some(person) = self
            .store
            .person_of_party(id)
            .await
            .map_err(|error| crate::facade::ingest::refusal::store_refusal(&error))?
        {
            return Ok(Some(person));
        }
        // NOTE: an id that cannot be a party uid legitimately names no party.
        let Ok(uid) = HierObjectId::new(id) else {
            return Ok(None);
        };
        let class = loaded.program().party().ok_or_else(|| {
            Refused::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception).diagnosing(format!(
                    "the context {} is served as a party and compiled no party context",
                    loaded.program().context()
                )),
            )
        })?;
        let Some(held) = crate::facade::party::find(
            crate::facade::party::demographic(self.client)?,
            class,
            uid.value(),
        )
        .await?
        else {
            return Ok(None);
        };
        let accepted = crate::facade::party::accept(loaded, PATIENT, id, &held)?;
        let resource =
            crate::facade::party::render(self.renders, loaded, &accepted, &held.version).await?;
        let subject = self
            .link
            .patient(&Value::Object(resource))
            .map_err(|error| {
                Refused::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Issue::error(IssueType::Required)
                        .diagnosing(ferrohealth_server::run::chain(&error))
                        .at(format!("{PATIENT}.identifier")),
                )
            })?;
        let Subject::Identified(source) = subject else {
            return Ok(None);
        };
        let person = self.pseudonym(&source).await?;
        self.store
            .record_party(id, &person)
            .await
            .map_err(|error| crate::facade::ingest::refusal::store_refusal(&error))?;
        Ok(Some(person))
    }

    /// Asks the identity source for the pseudonym of `source`.
    async fn pseudonym(&self, source: &SourceIdentifier) -> Result<PersonId, Refused> {
        self.link
            .identity()
            .pseudonym(source, &self.on_behalf)
            .await
            .map_err(|error| refused(&error))
    }
}

/// Returns the literal `Patient` reference `person` is read from: the party
/// the identity map records it for under a `Patient` party context, its
/// pseudonym elsewhere; `None` when there is none.
///
/// # Errors
///
/// Returns a `500` when the identity map cannot be read.
pub async fn reference_of(
    link: &SubjectLink,
    store: &dyn Store,
    programs: &Programs,
    person: &PersonId,
) -> Result<Option<String>, Refused> {
    if programs.party(PATIENT).is_none() {
        return Ok(link.pseudonym_reference(person));
    }
    Ok(store
        .party_of(person)
        .await
        .map_err(|error| crate::facade::ingest::refusal::store_refusal(&error))?
        .map(|party| format!("{PATIENT}/{party}")))
}

/// Returns a `422` of `kind` diagnosing `text`.
fn unprocessable(kind: IssueType, text: String) -> Refused {
    Refused::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        Issue::error(kind).diagnosing(text),
    )
}

/// Returns whom the identity source is asked for on behalf of: the caller
/// `access` names, or the bridge itself on an open surface.
#[must_use]
pub fn on_behalf(access: &ferrohealth_smart::resource::Access) -> OnBehalfOf {
    access.caller().map_or(OnBehalfOf::System, |caller| {
        OnBehalfOf::User(
            ihe_atna::user::User::local(caller.subject().to_owned())
                .with_issuer(caller.issuer().map(str::to_owned))
                .with_client_id(caller.client_id().map(str::to_owned)),
        )
    })
}

/// Returns the refusal a failed pseudonym query answers with.
///
/// It is a `422` when the identity source answered and holds no single
/// pseudonym, a `503` when a repeat may succeed, and a `502` for any other
/// upstream failure, each naming the upstream status it answered.
// NOTE: R4 http.html leaves an upstream failure to the server; no specification governs this:
// our own design, never an EHR under another subject.
#[must_use]
pub fn refused(error: &PseudonymError) -> Refused {
    let text = ferrohealth_server::run::chain(error);
    let text = match error.upstream_status() {
        Some(status) => format!("{text} (the identity source answered {status})"),
        None => text,
    };
    let (status, kind) = match error {
        PseudonymError::Upstream(_) if error.transient() => {
            (StatusCode::SERVICE_UNAVAILABLE, IssueType::Transient)
        }
        PseudonymError::Upstream(_) => (StatusCode::BAD_GATEWAY, IssueType::Exception),
        _ => (StatusCode::UNPROCESSABLE_ENTITY, IssueType::Processing),
    };
    Refused::new(status, Issue::error(kind).diagnosing(text))
}
