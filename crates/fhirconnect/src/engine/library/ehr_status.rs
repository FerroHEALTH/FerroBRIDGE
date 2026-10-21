// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ehrStatusAddExternalREF`: the identifiers of a FHIR resource, offered as
//! the subject reference of the EHR the composition goes into.
//!
//! The published library maps `$resource.identifier` onto `$reference` with
//! it, "these identifiers of FHIR need to go to the EHR status ... as
//! external references" (`projects/org.highmed/KDS/person/KDS_admin_entry_person.yml`).
//! `EHR_STATUS.subject.external_ref` "can be used to contain a direct
//! reference to the subject in a demographic or identity service" (openEHR
//! RM EHR IM Release 1.1.0, the `EHR_STATUS` class), so the function writes no
//! template node: it hands each `Identifier` to the run's subject channel
//! ([`crate::engine::subject`]) and the caller that commits picks the one the
//! deployment names. An identifier's `use`, `type`, `period` and `assigner`
//! select it through the mapping's condition and have no place in a
//! `PARTY_REF`. Coming out of openEHR it produces nothing, since a run reads
//! a composition and never the `EHR_STATUS`. No specification defines the
//! function: our own design.

use fhir_types::codec::Value;
use fhir_types::r4::identifier::Identifier;

use crate::engine::library::decode;
use crate::engine::library::primitive;
use crate::engine::library::refused;
use crate::engine::subject::SubjectIdentifier;
use crate::engine::traverse::functions::MappingCodeError;
use crate::engine::traverse::functions::Ports;

/// The name the published library's `mappingCode` writes.
pub const EXTERNAL_REF: &str = "ehrStatusAddExternalREF";

/// Offers one `Identifier` as the subject reference of the EHR.
///
/// Reads `Identifier.system` and `Identifier.value`
/// (<https://hl7.org/fhir/R4/datatypes.html#Identifier>).
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the element is no `Identifier`, when it
/// carries no `value`, which a reference cannot do without, or when the
/// `value` or the `system` carries an id or an extension.
pub fn to_subject(element: &Value) -> Result<Vec<SubjectIdentifier>, MappingCodeError> {
    let code = EXTERNAL_REF;
    let identifier: Identifier = decode(code, "Identifier", element)?;
    let value = identifier.value.as_ref().ok_or_else(|| {
        refused(
            code,
            "the identifier carries no `value`, so it names no subject",
        )
    })?;
    let value = primitive(
        code,
        "Identifier.value",
        value.id.as_ref(),
        &value.extension,
        value.value.as_ref(),
    )?;
    let system = identifier
        .system
        .as_ref()
        .map(|system| {
            primitive(
                code,
                "Identifier.system",
                system.id.as_ref(),
                &system.extension,
                system.value.as_ref(),
            )
        })
        .transpose()?
        .map(String::from);
    Ok(vec![SubjectIdentifier::new(system, value)])
}

/// Refuses a call through the template-node direction, which this function
/// has no port for.
///
/// # Errors
///
/// Always returns [`MappingCodeError::Refused`]: the engine runs the function
/// through [`to_subject`].
pub fn to_openehr(_element: &Value) -> Result<Ports, MappingCodeError> {
    Err(refused(
        EXTERNAL_REF,
        "the function writes the subject of the EHR, not a template node",
    ))
}

/// Refuses a call out of openEHR, where the function has nothing to read.
///
/// # Errors
///
/// Always returns [`MappingCodeError::Refused`]: the engine declares the
/// mapping skipped going out, since the subject lives in the `EHR_STATUS`.
pub fn to_fhir(_ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
    Err(refused(
        EXTERNAL_REF,
        "the subject of the EHR lives in the EHR_STATUS, which a composition does not hold",
    ))
}
