// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The link between a FHIR subject and the EHR it is written into.
//!
//! An EHR names its subject in `EHR_STATUS.subject.external_ref`, a
//! `PARTY_REF` into an identity service (RM Common IM §4.2.1.1,
//! <https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html>).
//! Every subject is the patient's pseudonym in the identity source's
//! pseudonym domain, asked over ITI-83 (PIXm 3.1.0 §2:3.83.4.1.2.2,
//! <https://profiles.ihe.net/ITI/PIXm/3.1.0/ITI-83.html>), so no direct
//! identifier reaches the CDR (GDPR Art 4(5), Art 25(1)). The facade's
//! `Patient` logical id is that pseudonym.

use std::sync::Arc;

use ihe_atna_next::user::OnBehalfOf;
use ihe_iti::pixm::identifier::SourceIdentifier;

use crate::facade::identity::IdError;
use crate::facade::identity::PersonId;
use crate::identity_source::{IdentitySource, PseudonymError, SourceError};

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

/// A subject as a request names it, before the identity source is asked.
#[derive(Debug, Clone)]
pub enum Subject {
    /// The pseudonym itself, from a `Patient/<id>` reference.
    Person(PersonId),
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

    /// Returns the person a literal `Patient/<pseudonym>` reference names.
    ///
    /// # Errors
    ///
    /// Returns [`SubjectError::NotPatient`] for any other reference and
    /// [`SubjectError::Key`] when the id cannot be a key.
    pub fn person_of_reference(&self, literal: &str) -> Result<PersonId, SubjectError> {
        let id = logical_id(literal).ok_or(SubjectError::NotPatient)?;
        PersonId::new(self.namespace(), id).map_err(|source| SubjectError::Key { source })
    }

    /// Returns the person the patient with logical id `patient` is.
    ///
    /// # Errors
    ///
    /// Returns the refusals of [`SubjectLink::person_of_reference`].
    pub fn person_of_patient(&self, patient: &str) -> Result<PersonId, SubjectError> {
        self.person_of_reference(&format!("{PATIENT}/{patient}"))
    }

    /// Returns the literal reference `person` is read from, `None` for a
    /// person keyed in another namespace.
    #[must_use]
    pub fn reference_of(&self, person: &PersonId) -> Option<String> {
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
    pub fn patient(&self, document: &fhir_types::codec::Value) -> Result<Subject, SubjectError> {
        let identifiers: Vec<(Option<&str>, &str)> = document
            .get("identifier")
            .and_then(fhir_types::codec::Value::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(|identifier| {
                let value = identifier
                    .get("value")
                    .and_then(fhir_types::codec::Value::as_str)?;
                let system = identifier
                    .get("system")
                    .and_then(fhir_types::codec::Value::as_str);
                Some((system, value))
            })
            .collect();
        Ok(Subject::Identified(self.identity.source_of(&identifiers)?))
    }

    /// Returns the person `subject` is, asking the identity source for the
    /// pseudonym of an identified subject on behalf of `on_behalf`.
    ///
    /// # Errors
    ///
    /// Returns the [`PseudonymError`] of the identity source.
    pub async fn person(
        &self,
        subject: Subject,
        on_behalf: &OnBehalfOf,
    ) -> Result<PersonId, PseudonymError> {
        match subject {
            Subject::Person(person) => Ok(person),
            Subject::Identified(source) => self.identity.pseudonym(&source, on_behalf).await,
        }
    }
}

/// Returns whom the identity source is asked for on behalf of: the caller
/// `access` names, or the bridge itself on an open surface.
#[must_use]
pub fn on_behalf(access: &ferrohealth_smart::resource::Access) -> OnBehalfOf {
    access.caller().map_or(OnBehalfOf::System, |caller| {
        OnBehalfOf::User(
            ihe_atna_next::user::User::local(caller.subject().to_owned())
                .with_issuer(caller.issuer().map(str::to_owned))
                .with_client_id(caller.client_id().map(str::to_owned)),
        )
    })
}

/// Returns the refusal a failed pseudonym query answers with: a `422` when
/// the identity source answered and holds no single pseudonym, a `503` when
/// a repeat may succeed, and a `502` for any other upstream failure, each
/// naming the upstream status it answered.
// NOTE: R4 http.html leaves an upstream failure to the server; no specification governs this:
// our own design, never an EHR under another subject.
#[must_use]
pub fn refused(error: &PseudonymError) -> crate::facade::ingest::Refused {
    use crate::facade::outcome::{Issue, IssueType};
    use http::StatusCode;

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
    crate::facade::ingest::Refused::new(status, Issue::error(kind).diagnosing(text))
}

/// Returns the logical id of a relative `Patient/<id>` reference.
fn logical_id(literal: &str) -> Option<&str> {
    let id = literal.strip_prefix(PATIENT)?.strip_prefix('/')?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}
