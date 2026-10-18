// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The link between a FHIR subject and the EHR it is written into.
//!
//! An EHR names its subject in `EHR_STATUS.subject.external_ref`, a
//! `PARTY_REF` into a demographic or identity service (RM Common IM §4.2.1.1,
//! <https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html>),
//! and the facade finds the EHR again through the ITS-REST lookup by subject
//! id and namespace (`ehr-codegen.openapi.yaml`, `ehr_get_by_subject`). The
//! namespace, the `PARTY_REF.type` and the form of the id are configuration,
//! because the link has to match what the CDR's owner writes: BASE leaves
//! namespaces "not yet standardised", so the namespace and the id form are
//! our own design. The facade never derives an id the CDR mints itself, such
//! as a pseudonym; a deployment whose CDR holds pseudonyms names its subjects
//! by the id the CDR already linked.

use openehr_base::v1_3::base_types::identification::generic_id::GenericId;
use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
use openehr_base::v1_3::base_types::identification::party_ref::PartyRef;
use openehr_base::validate::InvariantViolation;
use openehr_base::validate::Validate;

use crate::facade::identity::IdError;
use crate::facade::identity::PersonId;

/// The resource type a literal subject reference names.
const PATIENT: &str = "Patient";

/// How the id half of a subject is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IdForm {
    /// The literal reference as the resource writes it, `Patient/<id>`.
    #[default]
    Reference,
    /// The logical id of a `Patient/<id>` reference, the bare `<id>`.
    LogicalId,
    /// The logical id, which must be a UUID.
    Uuid,
}

impl IdForm {
    /// Returns the form a configuration value names, `None` for any other.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "reference" => Some(Self::Reference),
            "logical_id" => Some(Self::LogicalId),
            "uuid" => Some(Self::Uuid),
            _ => None,
        }
    }

    /// Returns the configuration value that names this form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::LogicalId => "logical_id",
            Self::Uuid => "uuid",
        }
    }
}

/// Why a subject link or a subject cannot be formed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SubjectError {
    /// The configured `PARTY_REF.type` breaks the BASE `Type_validity`
    /// invariant.
    #[error("`{value}` cannot be a PARTY_REF.type: {invariant}")]
    PartyType {
        /// The configured type.
        value: String,
        /// The invariant the `openehr-base` check reported.
        invariant: String,
    },
    /// The configured namespace breaks the `OBJECT_REF` namespace rule.
    #[error("`{value}` cannot be a PARTY_REF.namespace: {invariant}")]
    Namespace {
        /// The configured namespace.
        value: String,
        /// The invariant the `openehr-base` check reported.
        invariant: String,
    },
    /// A literal reference is not the `Patient/<id>` form the configured id
    /// form reads.
    #[error(
        "the subject reference is no relative `Patient/<id>` reference, which facade.subject_id_form = {} reads",
        form.as_str()
    )]
    NotPatient {
        /// The configured form.
        form: IdForm,
    },
    /// A `Patient` carries no logical id, which the configured id form keys
    /// it by.
    #[error(
        "the Patient carries no logical id, which facade.subject_id_form = {} keys its EHR by",
        form.as_str()
    )]
    NoPatientId {
        /// The configured form.
        form: IdForm,
    },
    /// A logical id is not a UUID, which the configured id form requires.
    #[error("the patient id is no UUID, which facade.subject_id_form = uuid requires")]
    NotUuid,
    /// The parts cannot be a person key.
    #[error("the subject cannot be a person key")]
    Key {
        /// What the person key's constructor reported.
        #[source]
        source: IdError,
    },
}

/// The configured link between a FHIR subject and its EHR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectLink {
    /// The namespace a literal subject reference is read in.
    namespace: String,
    /// How the id half is written.
    form: IdForm,
    /// The `PARTY_REF.type` an EHR the facade creates names.
    party_type: String,
}

impl SubjectLink {
    /// Returns the link, with the namespace and the type checked against the
    /// `PARTY_REF` invariants of the pinned `openehr-base`.
    ///
    /// # Errors
    ///
    /// Returns [`SubjectError::PartyType`] when `party_type` breaks
    /// `Type_validity` and [`SubjectError::Namespace`] when `namespace` breaks
    /// the `OBJECT_REF` namespace rule.
    pub fn new(namespace: &str, form: IdForm, party_type: &str) -> Result<Self, SubjectError> {
        // NOTE: BASE base_types `PARTY_REF` §Invariants `Type_validity`, checked by
        // `openehr-base`, which also admits the `ANY` its inherited `type` sanctions.
        if let Some(violation) = first(&party_ref("local", party_type)) {
            return Err(SubjectError::PartyType {
                value: party_type.to_owned(),
                invariant: violation.message,
            });
        }
        if let Some(violation) = first(&party_ref(namespace, "PERSON")) {
            return Err(SubjectError::Namespace {
                value: namespace.to_owned(),
                invariant: violation.message,
            });
        }
        Ok(Self {
            namespace: namespace.to_owned(),
            form,
            party_type: party_type.to_owned(),
        })
    }

    /// Returns the namespace a literal subject reference is read in.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Returns how the id half is written.
    #[must_use]
    pub fn form(&self) -> IdForm {
        self.form
    }

    /// Returns the `PARTY_REF.type` an EHR the facade creates names.
    #[must_use]
    pub fn party_type(&self) -> &str {
        &self.party_type
    }

    /// Returns the person a literal subject reference names.
    ///
    /// # Errors
    ///
    /// Returns [`SubjectError::NotPatient`] when the form reads a logical id
    /// and `literal` is no relative `Patient/<id>` reference,
    /// [`SubjectError::NotUuid`] when the form is [`IdForm::Uuid`] and the id
    /// is no UUID, and [`SubjectError::Key`] when the parts cannot be a key.
    pub fn person_of_reference(&self, literal: &str) -> Result<PersonId, SubjectError> {
        let id = match self.form {
            IdForm::Reference => literal,
            IdForm::LogicalId | IdForm::Uuid => {
                logical_id(literal).ok_or(SubjectError::NotPatient { form: self.form })?
            }
        };
        if self.form == IdForm::Uuid && uuid::Uuid::try_parse(id).is_err() {
            return Err(SubjectError::NotUuid);
        }
        PersonId::new(&self.namespace, id).map_err(|source| SubjectError::Key { source })
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
    /// person keyed in another namespace or by another form.
    #[must_use]
    pub fn reference_of(&self, person: &PersonId) -> Option<String> {
        if person.namespace() != self.namespace {
            return None;
        }
        match self.form {
            IdForm::Reference => person.id().contains('/').then(|| person.id().to_owned()),
            IdForm::LogicalId | IdForm::Uuid => {
                (!person.id().contains('/')).then(|| format!("{PATIENT}/{}", person.id()))
            }
        }
    }
}

/// Returns the logical id of a relative `Patient/<id>` reference.
fn logical_id(literal: &str) -> Option<&str> {
    let id = literal.strip_prefix(PATIENT)?.strip_prefix('/')?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

/// Returns a `PARTY_REF` of `party_type` in `namespace`.
fn party_ref(namespace: &str, party_type: &str) -> PartyRef {
    PartyRef {
        namespace: namespace.to_owned(),
        r#type: party_type.to_owned(),
        id: ObjectId::GenericId(GenericId {
            value: String::from("subject"),
            scheme: namespace.to_owned(),
        }),
    }
}

/// Returns the first `PARTY_REF` invariant `reference` breaks.
fn first(reference: &PartyRef) -> Option<InvariantViolation> {
    reference.invariants().into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::{IdForm, SubjectError, SubjectLink};

    /// A UUID invented for the tests.
    const UUID: &str = "0b5f0e3c-7d55-4c2e-9d0a-1f2e3d4c5b6a";

    #[test]
    fn every_type_validity_value_is_admitted() {
        for party_type in [
            "PERSON",
            "ORGANISATION",
            "GROUP",
            "AGENT",
            "ROLE",
            "PARTY",
            "ACTOR",
        ] {
            assert!(
                SubjectLink::new("ferrobridge", IdForm::Reference, party_type).is_ok(),
                "{party_type} is a Type_validity value"
            );
        }
    }

    #[test]
    fn a_type_outside_type_validity_is_refused() {
        let refused = SubjectLink::new("ferrobridge", IdForm::Reference, "PATIENT");
        assert!(matches!(
            refused,
            Err(SubjectError::PartyType { ref value, .. }) if value == "PATIENT"
        ));
    }

    #[test]
    fn a_type_is_compared_case_exact() {
        let refused = SubjectLink::new("ferrobridge", IdForm::Reference, "person");
        assert!(matches!(refused, Err(SubjectError::PartyType { .. })));
    }

    #[test]
    fn a_namespace_breaking_the_object_ref_rule_is_refused() {
        let refused = SubjectLink::new("1-local", IdForm::Reference, "PERSON");
        assert!(matches!(refused, Err(SubjectError::Namespace { .. })));
    }

    #[test]
    fn the_reference_form_keys_the_literal_reference() {
        let link = SubjectLink::new("ferrobridge", IdForm::Reference, "PERSON")
            .expect("a legal subject link");
        let person = link
            .person_of_reference("Patient/p-1")
            .expect("the link reads it");
        assert_eq!(
            ("ferrobridge", "Patient/p-1"),
            (person.namespace(), person.id())
        );
        assert_eq!(
            Some(String::from("Patient/p-1")),
            link.reference_of(&person)
        );
    }

    #[test]
    fn the_logical_id_form_keys_the_bare_id() {
        let link = SubjectLink::new("fhir_facade", IdForm::LogicalId, "PERSON")
            .expect("a legal subject link");
        let person = link
            .person_of_reference("Patient/p-1")
            .expect("the link reads it");
        assert_eq!(("fhir_facade", "p-1"), (person.namespace(), person.id()));
        assert_eq!(
            Some(String::from("Patient/p-1")),
            link.reference_of(&person)
        );
        assert_eq!(
            person,
            link.person_of_patient("p-1").expect("the link reads it")
        );
    }

    #[test]
    fn the_logical_id_form_refuses_a_reference_to_another_type() {
        let link = SubjectLink::new("fhir_facade", IdForm::LogicalId, "PERSON")
            .expect("a legal subject link");
        for literal in [
            "Group/g-1",
            "http://example.org/fhir/Patient/p-1",
            "Patient/",
            "Patient/p-1/_history/2",
        ] {
            assert_eq!(
                Err(SubjectError::NotPatient {
                    form: IdForm::LogicalId
                }),
                link.person_of_reference(literal),
                "{literal}"
            );
        }
    }

    #[test]
    fn the_uuid_form_keys_a_uuid_logical_id() {
        let link =
            SubjectLink::new("pseudonyms", IdForm::Uuid, "PERSON").expect("a legal subject link");
        let person = link
            .person_of_reference(&format!("Patient/{UUID}"))
            .expect("the link reads it");
        assert_eq!(("pseudonyms", UUID), (person.namespace(), person.id()));
    }

    #[test]
    fn the_uuid_form_refuses_an_id_that_is_no_uuid() {
        let link =
            SubjectLink::new("pseudonyms", IdForm::Uuid, "PERSON").expect("a legal subject link");
        let refused = link.person_of_reference("Patient/p-1");
        assert_eq!(Err(SubjectError::NotUuid), refused);
        let message = refused.expect_err("an id that is no UUID").to_string();
        assert!(message.contains("facade.subject_id_form"), "{message}");
    }

    #[test]
    fn a_person_of_another_namespace_has_no_literal_reference() {
        let link = SubjectLink::new("fhir_facade", IdForm::LogicalId, "PERSON")
            .expect("a legal subject link");
        let other = crate::facade::identity::PersonId::new("http://example.org/mrn", "p-1")
            .expect("a legal person id");
        assert_eq!(None, link.reference_of(&other));
    }

    #[test]
    fn every_form_round_trips_through_its_configuration_value() {
        for form in [IdForm::Reference, IdForm::LogicalId, IdForm::Uuid] {
            assert_eq!(Some(form), IdForm::parse(form.as_str()));
        }
        assert_eq!(None, IdForm::parse("pseudonym"));
    }
}
