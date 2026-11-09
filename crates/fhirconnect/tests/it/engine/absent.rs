// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A mapping out of openEHR whose node holds no value, beside a later
//! mapping onto the same repeating FHIR element.
//!
//! FHIRconnect appends to a `0..n` path it does not iterate, so each mapping
//! onto `Condition.note` writes a fresh note; one whose node holds no value
//! (the context end time, a family of the context node) takes none, and the
//! next writes the first. No specification governs the
//! instance numbering: our own design.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;

use crate::engine::defaults;
use crate::support::compiled;
use crate::support::template;

#[test]
fn a_mapping_with_no_value_out_of_openehr_takes_no_instance() -> Result<(), Box<dyn Error>> {
    let index = template()?;
    let program = compiled("ferrobridge_absent_first")?;
    let condition = Value::from_serde_json(serde_json::json!({
        "resourceType": "Condition",
        "code": {"text": "Synthetic problem"},
        "subject": {"reference": "Patient/synthetic-patient"}
    }));
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition,
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &Seams::default(),
        &CallContext::new(),
    )?;
    assert_eq!(
        outbound.value().get("note"),
        Some(&Value::from_serde_json(
            serde_json::json!([{"text": "Synthetic problem"}])
        ))
    );
    Ok(())
}
