// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `mappingCode` mapping onto `$reference` that offers the resource's
//! identifiers as the subject of the EHR: going in they travel beside the
//! composition (`Outcome::subject`), coming out the mapping is declared
//! skipped, since a run reads a composition and the subject lives in the
//! `EHR_STATUS` (openEHR RM EHR IM Release 1.1.0, the `EHR_STATUS` class).

use core::error::Error;
use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::library::Library;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::SkipReason;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::subject::SubjectIdentifier;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::index::WebTemplateIndex;
use serde_json::json;

use crate::engine::defaults;
use crate::support::compiled_models;
use crate::support::template;

/// The fixture pair.
const FIXTURE: &str = "ferrobridge_subject_identifier";

/// Compiles the fixture against the diagnosis template with the library.
fn program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = template()?;
    let program = compiled_models(FIXTURE, &[FIXTURE], &index, &Library)?;
    Ok((program, index))
}

/// Returns the synthetic `Condition` with `identifiers`.
fn condition(identifiers: &serde_json::Value) -> Result<Value, Box<dyn Error>> {
    let mut parsed: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)?;
    let object = parsed.as_object_mut().ok_or("the Condition is an object")?;
    object.insert(String::from("identifier"), identifiers.clone());
    Ok(Value::from_serde_json(parsed))
}

#[test]
fn every_identifier_is_offered_as_the_subject_going_in() -> Result<(), Box<dyn Error>> {
    let (program, index) = program()?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(&json!([
            {"system": "urn:oid:2.999.1", "value": "synthetic-pseudonym-1"},
            {"value": "synthetic-local-2"}
        ]))?,
        &Seams::default().with_functions(&Library),
        &defaults(),
        &CallContext::new(),
    )?;
    assert_eq!(
        inbound.subject(),
        [
            SubjectIdentifier::new(
                Some(String::from("urn:oid:2.999.1")),
                "synthetic-pseudonym-1"
            ),
            SubjectIdentifier::new(None, "synthetic-local-2"),
        ]
    );
    Ok(())
}

#[test]
fn a_resource_with_no_identifier_offers_no_subject() -> Result<(), Box<dyn Error>> {
    let (program, index) = program()?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(&json!([]))?,
        &Seams::default().with_functions(&Library),
        &defaults(),
        &CallContext::new(),
    )?;
    assert!(inbound.subject().is_empty());
    Ok(())
}

#[test]
fn coming_out_the_mapping_is_declared_skipped() -> Result<(), Box<dyn Error>> {
    let (program, index) = program()?;
    let seams = Seams::default().with_functions(&Library);
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(&json!([{"value": "synthetic-local-2"}]))?,
        &seams,
        &defaults(),
        &CallContext::new(),
    )?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &seams,
        &CallContext::new(),
    )?;
    assert!(
        outbound.warnings().iter().any(|warning| matches!(
            warning.loss(),
            Loss::Skipped {
                reason: SkipReason::EhrStatusSubject,
                ..
            }
        )),
        "{:?}",
        outbound.warnings()
    );
    Ok(())
}
