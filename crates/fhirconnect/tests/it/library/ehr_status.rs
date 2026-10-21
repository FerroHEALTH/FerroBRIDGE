// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ehrStatusAddExternalREF`: an `Identifier` offered as the subject of the
//! EHR (openEHR RM EHR IM Release 1.1.0, `EHR_STATUS.subject`), and nothing
//! coming out.

use fhirconnect::engine::library::Library;
use fhirconnect::engine::subject::SubjectIdentifier;
use fhirconnect::engine::traverse::functions::MappingCodeError;
use fhirconnect::engine::traverse::functions::MappingFunctions;
use fhirconnect::engine::traverse::functions::Ports;
use fhirconnect::model::semantic::MappingCodeRegistry;
use serde_json::json;

use super::element;

/// The name the published library writes.
const CODE: &str = "ehrStatusAddExternalREF";

#[test]
fn the_function_is_registered_with_no_port_and_writes_the_subject() {
    assert!(Library.contains(CODE));
    assert_eq!(Library.ports(CODE), Some(&[][..]));
    assert!(Library.writes_subject(CODE));
    assert!(!Library.writes_subject("timingNonDaily"));
}

#[test]
fn an_identifier_is_offered_with_its_system_and_value() -> Result<(), MappingCodeError> {
    let offered = Library.to_subject(
        CODE,
        &element(json!({
            "use": "official",
            "type": {"coding": [{"code": "PSEUDED"}]},
            "system": "urn:oid:2.999.1",
            "value": "0b5f0e3c-7d55-4c2e-9d0a-1f2e3d4c5b6a"
        })),
    )?;
    assert_eq!(
        offered,
        [SubjectIdentifier::new(
            Some(String::from("urn:oid:2.999.1")),
            "0b5f0e3c-7d55-4c2e-9d0a-1f2e3d4c5b6a"
        )]
    );
    Ok(())
}

#[test]
fn an_identifier_with_no_system_is_offered_without_one() -> Result<(), MappingCodeError> {
    let offered = Library.to_subject(CODE, &element(json!({"value": "synthetic-42"})))?;
    assert_eq!(offered, [SubjectIdentifier::new(None, "synthetic-42")]);
    Ok(())
}

#[test]
fn an_identifier_with_no_value_names_no_subject_and_is_refused() {
    let refused = Library.to_subject(CODE, &element(json!({"system": "urn:oid:2.999.1"})));
    assert!(
        matches!(refused, Err(MappingCodeError::Refused { .. })),
        "{refused:?}"
    );
}

#[test]
fn neither_template_direction_runs() {
    assert!(matches!(
        Library.to_openehr(CODE, &element(json!({"value": "synthetic-42"}))),
        Err(MappingCodeError::Refused { .. })
    ));
    assert!(matches!(
        Library.to_fhir(CODE, &Ports::new()),
        Err(MappingCodeError::Refused { .. })
    ));
}
