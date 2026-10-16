// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `Condition.recorder` written into `COMPOSITION.composer`, and the default
//! composer of `engine/defaults-for-fields.adoc` when the recorder carries
//! nothing a `PARTY_IDENTIFIED` holds.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::Outcome;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::composition::CanonicalComposition;

use crate::support::compiled;
use crate::support::template;

/// Runs the composer fixture over the synthetic `Condition` with `recorder`.
fn run(
    recorder: Option<serde_json::Value>,
) -> Result<Outcome<CanonicalComposition>, Box<dyn Error>> {
    let mut condition: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)?;
    if let (Some(object), Some(recorder)) = (condition.as_object_mut(), recorder) {
        object.insert(String::from("recorder"), recorder);
    }
    Ok(to_openehr(
        &*compiled("ferrobridge_composer")?,
        &SCHEMAS,
        &template()?,
        &Value::from_serde_json(condition),
        &Seams::default(),
        &Defaults::at("2026-10-16T08:00:00Z")
            .with_language("en")
            .with_territory("NL"),
        &CallContext::new(),
    )?)
}

/// Returns the composer of the built composition.
fn composer(outcome: Outcome<CanonicalComposition>) -> serde_json::Value {
    outcome
        .into_value()
        .into_value()
        .get("composer")
        .cloned()
        .unwrap_or_default()
}

/// Returns whether the run filled the composer with its default.
fn defaulted(outcome: &Outcome<CanonicalComposition>) -> bool {
    outcome
        .warnings()
        .iter()
        .any(|warning| matches!(warning.loss(), Loss::Defaulted { field } if field == "/composer"))
}

#[test]
fn a_recorder_with_a_display_names_the_composer() -> Result<(), Box<dyn Error>> {
    let outcome = run(Some(serde_json::json!({
        "reference": "Practitioner/synthetic-practitioner-1",
        "display": "Synthetic Recorder"
    })))?;
    assert!(!defaulted(&outcome));
    assert_eq!(
        composer(outcome).get("name"),
        Some(&serde_json::json!("Synthetic Recorder"))
    );
    Ok(())
}

#[test]
fn a_recorder_with_only_a_reference_leaves_the_default_composer() -> Result<(), Box<dyn Error>> {
    let outcome = run(Some(serde_json::json!({
        "reference": "Practitioner/synthetic-practitioner-1"
    })))?;
    assert!(defaulted(&outcome));
    assert_eq!(
        composer(outcome).get("name"),
        Some(&serde_json::json!("FHIRconnect"))
    );
    Ok(())
}

#[test]
fn no_recorder_leaves_the_default_composer() -> Result<(), Box<dyn Error>> {
    let outcome = run(None)?;
    assert!(defaulted(&outcome));
    assert_eq!(
        composer(outcome).get("name"),
        Some(&serde_json::json!("FHIRconnect"))
    );
    Ok(())
}
