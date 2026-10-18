// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Holding a `patient/` token to the patient in its launch context.
//!
//! "Patient-specific scopes allow access to specific data about a single
//! patient", the one the `patient` launch context names
//! (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
//! On this facade a patient's data is the EHR the facade resolves for the
//! subject `Patient/<id>`: a write's subject must be that reference, and a
//! read reaches only a composition of that EHR. A subject named by
//! identifier, or by another literal form, cannot be shown to be the patient,
//! so a patient-scoped token is refused for it (no specification governs
//! the mapping from a FHIR compartment to an openEHR EHR: our own design).
//! A refusal is the same `403` the guard answers, naming no scope.

use http::StatusCode;

use crate::auth::Access;
use crate::auth::Compartment;
use crate::auth::scope::Permission;
use crate::cdr::ids::EhrId;
use crate::facade::Facade;
use crate::facade::identity::PersonId;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::reply::Refusal;
use crate::facade::request::Inbound;

/// Checks that `access` lets `permission` on `resource_type` reach the data
/// of `person`, the subject the facade resolved, `None` when it resolved none.
///
/// # Errors
///
/// Returns the `403` refusal when the token grants no such permission, or
/// grants it for another patient's compartment only.
pub fn admit_person(
    facade: &Facade,
    access: &Access,
    permission: Permission,
    resource_type: &str,
    person: Option<&PersonId>,
) -> Result<(), Refusal> {
    match access.reach(permission, resource_type) {
        Some(Compartment::Any) => Ok(()),
        Some(Compartment::Patient(patient)) => {
            // NOTE: no specification governs this: our own design, a patient id the
            // subject link cannot read is no person, so it admits nothing.
            let launched = facade.settings().subject.person_of_patient(&patient).ok();
            let admitted = person.is_some_and(|person| launched.as_ref() == Some(person));
            if admitted {
                Ok(())
            } else {
                Err(crate::auth::guard::forbidden())
            }
        }
        None => Err(crate::auth::guard::forbidden()),
    }
}

/// Checks that `access` lets `permission` reach the subject `inbound` names.
///
/// # Errors
///
/// Returns the `403` refusal of [`admit_person`].
pub fn admit_inbound(
    facade: &Facade,
    access: &Access,
    permission: Permission,
    inbound: &Inbound,
) -> Result<(), Refusal> {
    if matches!(
        access.reach(permission, inbound.resource_type()),
        Some(Compartment::Any)
    ) {
        return Ok(());
    }
    // NOTE: a resource naming no usable subject is legitimately outside every
    // patient's compartment, so the failed lookup becomes the refusal below.
    let person = inbound.subject(&facade.settings().subject).ok();
    admit_person(
        facade,
        access,
        permission,
        inbound.resource_type(),
        person.as_ref(),
    )
}

/// Checks that `access` lets `permission` on `resource_type` reach the data
/// of the EHR `ehr`, through the subject the identity map records for it.
///
/// # Errors
///
/// Returns the `403` refusal of [`admit_person`], and a `500` when the
/// identity map cannot be read.
pub fn admit_ehr(
    facade: &Facade,
    access: &Access,
    permission: Permission,
    resource_type: &str,
    ehr: &EhrId,
) -> Result<(), Refusal> {
    if matches!(
        access.reach(permission, resource_type),
        Some(Compartment::Any)
    ) {
        return Ok(());
    }
    let person = facade.store().person_of(ehr).map_err(|error| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(format!(
                "the identity map could not be read: {}",
                crate::facade::outcome::chain(&error)
            )),
        )
    })?;
    admit_person(facade, access, permission, resource_type, person.as_ref())
}
