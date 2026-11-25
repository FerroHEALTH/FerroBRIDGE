// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identifier type a patient party carries under the draft `EEHRxF`
//! Annex X.
//!
//! Draft Annex X (Have your say initiative 17512, 2026-11-23) gives
//! `EHDSPatient.identifier.type` the cardinality 1..1, and HL7 Europe Base and
//! Core 2.0.1 `patient-eu-core` leaves it optional. So a party context of
//! that profile refuses a write whose identifier carries no type, which the
//! mapping would otherwise commit untyped. The type's codes are HL7 v2-0203
//! for NI and SS and the `FerroTERM` code system
//! `https://ferroterm.eu/fhir/CodeSystem/ehds-identifier-type` for HI, HIN,
//! REGPID and REGION, for which no code system is published. No
//! specification governs the refusal or the code system: our own design.

use fhir_types::codec::Value;
use http::StatusCode;

use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::programs::Loaded;
use crate::facade::reply;
use crate::facade::reply::Refusal;

/// The profile the Annex X `EHDSPatient` group is served with.
pub(crate) const PATIENT_EU_CORE: &str =
    "http://hl7.eu/fhir/base/StructureDefinition/patient-eu-core";

/// Refuses a `Patient` of the `patient-eu-core` context `loaded` whose
/// identifier carries no type.
///
/// # Errors
///
/// Returns the `422` that names the first identifier without a type.
pub(crate) fn typed_identifiers(loaded: &Loaded, document: &Value) -> Result<(), Refusal> {
    if loaded.program().profile().url().as_str() != PATIENT_EU_CORE {
        return Ok(());
    }
    let identifiers = document
        .get("identifier")
        .and_then(Value::as_array)
        .unwrap_or_default();
    let Some(untyped) = identifiers
        .iter()
        .position(|identifier| identifier.get("type").is_none())
    else {
        return Ok(());
    };
    Err(reply::refusal(
        StatusCode::UNPROCESSABLE_ENTITY,
        Issue::error(IssueType::Required)
            .diagnosing(String::from(
                "draft EEHRxF Annex X gives EHDSPatient.identifier.type the cardinality 1..1: \
                 each identifier names its type",
            ))
            .at(format!("Patient.identifier[{untyped}].type")),
    ))
}
