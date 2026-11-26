// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The link between a FHIR subject and the EHR it is written into.
//!
//! An EHR names its subject in `EHR_STATUS.subject.external_ref`, a
//! `PARTY_REF` into an identity service (RM Common IM §4.2.1.1,
//! <https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html>).
//! Every subject is the patient's pseudonym in the identity source's
//! pseudonym domain, asked over ITI-83 (`PIXm` 3.1.0 §2:3.83.4.1.2.2,
//! <https://profiles.ihe.net/ITI/PIXm/3.1.0/ITI-83.html>), so no direct
//! identifier reaches the CDR (GDPR Art 4(5), Art 25(1)). The `Patient`
//! logical id is the party's `versioned_object_uid` where a `Patient` party
//! context is loaded, which the identity map records against the pseudonym,
//! and the pseudonym itself elsewhere.

use std::sync::Arc;

use fhir_types::codec::Value;
use fhirconnect::engine::demographics::Demographics;
use http::StatusCode;
use ihe_atna::user::OnBehalfOf;
use ihe_iti::pixm::identifier::SourceIdentifier;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;

use crate::cdr::CdrClient;
use crate::facade::identity::IdError;
use crate::facade::identity::PersonId;
use crate::facade::identity::store::Store;
use crate::facade::ingest::Refused;
use crate::facade::outcome::{Issue, IssueType};
use crate::facade::programs::Programs;
use crate::identity_source::{IdentitySource, PseudonymError, SourceError};
use crate::terminology::Translator;

/// The resource type a literal subject reference names.
const PATIENT: &str = "Patient";

/// The `PARTY_REF.type` of every EHR subject.
pub const PARTY_TYPE: &str = "PERSON";

/// Why a subject cannot be formed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SubjectError {
    /// A literal reference is no relative `Patient/<id>` reference.
    #[error("the subject reference is no relative `Patient/<id>` reference")]
    NotPatient,
    /// The subject names no identifier the identity source may be asked
    /// about.
    #[error(transparent)]
    Source(#[from] SourceError),
    /// The parts cannot be a person key.
    #[error("the subject cannot be a person key")]
    Key {
        /// What the person key's constructor reported.
        #[source]
        source: IdError,
    },
}

/// A subject as a request names it, before it is resolved to a person.
#[derive(Debug, Clone)]
pub enum Subject {
    /// A `Patient/<id>` reference, by the `Patient`'s logical id.
    Patient(String),
    /// An identifier, whose pseudonym the identity source holds.
    Identified(SourceIdentifier),
}

/// The link between a FHIR subject and its EHR, over the identity source.
#[derive(Debug, Clone)]
pub struct SubjectLink {
    /// The identity source the pseudonyms come from.
    identity: Arc<IdentitySource>,
}

impl SubjectLink {
    /// Returns the link over `identity`.
    #[must_use]
    pub const fn new(identity: Arc<IdentitySource>) -> Self {
        Self { identity }
    }

    /// Returns the pseudonym domain, the namespace of every EHR subject.
    #[must_use]
    pub fn namespace(&self) -> &str {
        self.identity.pseudonym_domain()
    }

    /// Returns the identity source.
    #[must_use]
    pub fn identity(&self) -> &IdentitySource {
        &self.identity
    }

    /// Returns the person whose pseudonym is `id`.
    ///
    /// # Errors
    ///
    /// Returns [`SubjectError::Key`] when `id` cannot be a key.
    pub fn person_of_pseudonym(&self, id: &str) -> Result<PersonId, SubjectError> {
        PersonId::new(self.namespace(), id).map_err(|source| SubjectError::Key { source })
    }

    /// Returns the `Patient/<pseudonym>` reference of `person`, `None` for a
    /// person keyed in another namespace.
    #[must_use]
    pub fn pseudonym_reference(&self, person: &PersonId) -> Option<String> {
        (person.namespace() == self.namespace() && !person.id().contains('/'))
            .then(|| format!("{PATIENT}/{}", person.id()))
    }

    /// Returns the subject a reference by identifier names.
    ///
    /// # Errors
    ///
    /// Returns [`SubjectError::Source`] when the identifier carries no system
    /// or one the deployment does not accept.
    pub fn identified(&self, system: Option<&str>, value: &str) -> Result<Subject, SubjectError> {
        Ok(Subject::Identified(self.identity.source(system, value)?))
    }

    /// Returns the subject the Patient `document` is: its identifier in the
    /// first accepted domain.
    ///
    /// # Errors
    ///
    /// Returns [`SubjectError::Source`] when it carries none.
    pub fn patient(&self, document: &Value) -> Result<Subject, SubjectError> {
        Ok(Subject::Identified(
            self.identity.source_of(&identifiers_of(document))?,
        ))
    }
}

/// Returns the system and the value of every `identifier` of `document`
/// that carries a `value`, in document order.
#[must_use]
pub fn identifiers_of(document: &Value) -> Vec<(Option<&str>, &str)> {
    document
        .get("identifier")
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|identifier| {
            let value = identifier.get("value").and_then(Value::as_str)?;
            let system = identifier.get("system").and_then(Value::as_str);
            Some((system, value))
        })
        .collect()
}

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
            .identity
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

/// Returns the subject a literal `Patient/<id>` reference names.
///
/// # Errors
///
/// Returns [`SubjectError::NotPatient`] for any other reference.
pub fn patient_of_reference(literal: &str) -> Result<Subject, SubjectError> {
    logical_id(literal)
        .map(|id| Subject::Patient(id.to_owned()))
        .ok_or(SubjectError::NotPatient)
}

/// Returns the logical id of a relative `Patient/<id>` reference.
fn logical_id(literal: &str) -> Option<&str> {
    let id = literal.strip_prefix(PATIENT)?.strip_prefix('/')?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}
