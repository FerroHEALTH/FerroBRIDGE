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
use ihe_iti::pixm::identifier::SourceIdentifier;

use crate::identity::IdError;
use crate::identity::PersonId;
use crate::identity_source::{IdentitySource, SourceError};

/// The resource type a literal subject reference names.
pub const PATIENT: &str = "Patient";

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

    /// Returns a handle on the identity source, for a lane that outlives one
    /// request.
    #[must_use]
    pub fn identity_handle(&self) -> Arc<IdentitySource> {
        Arc::clone(&self.identity)
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
