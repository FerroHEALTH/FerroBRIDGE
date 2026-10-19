// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `Condition.recorder` written into `COMPOSITION.composer`, and the default
//! composer of `engine/defaults-for-fields.adoc` when the recorder carries
//! nothing a `PARTY_IDENTIFIED` holds, and a composer reference beside a
//! participation that keeps its own reference.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::demographics::IssuerDemographics;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::Outcome;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::composition::CanonicalComposition;

use crate::support::compiled;
use crate::support::template;

/// The FHIR server base the bridge mints demographic identities under.
const BASE: &str = "https://bridge.example/fhir";

/// Runs the composer fixture over the synthetic `Condition` with `recorder`,
/// resolving party references as the FHIR server at [`BASE`].
fn run(
    recorder: Option<serde_json::Value>,
) -> Result<Outcome<CanonicalComposition>, Box<dyn Error>> {
    let demographics = IssuerDemographics::new(BASE)?;
    run_with(recorder, &Seams::default().with_demographics(&demographics))
}

/// Runs the composer fixture over the synthetic `Condition` with `recorder`
/// under `seams`.
fn run_with(
    recorder: Option<serde_json::Value>,
    seams: &Seams<'_>,
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
        seams,
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
fn a_recorder_reference_reaches_the_composer_as_its_external_ref() -> Result<(), Box<dyn Error>> {
    // RM Common IM, PARTY_PROXY.external_ref: "Optional reference to more detailed demographic
    // or identification information for this party, in an external system".
    let outcome = run(Some(serde_json::json!({
        "reference": "Practitioner/synthetic-practitioner-1",
        "display": "Synthetic Recorder"
    })))?;
    let external_ref = composer(outcome)
        .get("external_ref")
        .cloned()
        .ok_or("the composer carries the reference")?;
    assert_eq!(
        external_ref.get("namespace"),
        Some(&serde_json::json!(BASE))
    );
    assert_eq!(
        external_ref.pointer("/id/value"),
        Some(&serde_json::json!("Practitioner/synthetic-practitioner-1"))
    );
    assert_eq!(
        external_ref.pointer("/id/scheme"),
        Some(&serde_json::json!("fhir-reference"))
    );
    Ok(())
}

#[test]
fn a_recorder_reference_with_no_demographics_endpoint_refuses_the_run() -> Result<(), Box<dyn Error>>
{
    // engine/demographics.adoc: the reference is resolved, never dropped; with no endpoint the
    // run fails closed.
    let refused = run_with(
        Some(serde_json::json!({"reference": "Practitioner/synthetic-practitioner-1"})),
        &Seams::default(),
    );
    let Err(error) = refused else {
        return Err("a reference with no demographics endpoint was mapped".into());
    };
    assert!(
        error.to_string().contains("demographics endpoint"),
        "{error}"
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
/// The FHIR server base of a party reference the bridge did not mint.
const OTHER_BASE: &str = "https://other.example/fhir";

#[test]
fn a_composer_reference_leaves_another_party_its_own_namespace_and_scheme()
-> Result<(), Box<dyn Error>> {
    // Simplified Formats master06 §ID Namespace and Scheme: `ctx/id_namespace`
    // and `ctx/id_scheme` are the defaults for external references, so a
    // participation that writes its own keeps them.
    let demographics = IssuerDemographics::new(BASE)?;
    let mut condition: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)?;
    if let Some(object) = condition.as_object_mut() {
        object.insert(
            String::from("recorder"),
            serde_json::json!({"reference": "Practitioner/synthetic-practitioner-1"}),
        );
        object.insert(
            String::from("asserter"),
            serde_json::json!({
                "reference": format!("{OTHER_BASE}/Practitioner/synthetic-practitioner-2")
            }),
        );
    }
    let index = template()?;
    let outcome = to_openehr(
        &*compiled("ferrobridge_composer_participation")?,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(condition),
        &Seams::default().with_demographics(&demographics),
        &Defaults::at("2026-10-16T08:00:00Z")
            .with_language("en")
            .with_territory("NL"),
        &CallContext::new(),
    )?;
    let composition = outcome.into_value().into_value();
    let composer = composition
        .pointer("/composer/external_ref")
        .cloned()
        .ok_or("the composer carries its reference")?;
    assert_eq!(composer.get("namespace"), Some(&serde_json::json!(BASE)));
    assert_eq!(
        composer.pointer("/id/scheme"),
        Some(&serde_json::json!("fhir-reference"))
    );
    let performer = composition
        .pointer("/context/participations/0/performer/external_ref")
        .cloned()
        .ok_or("the participation carries its performer's reference")?;
    assert_eq!(
        performer.get("namespace"),
        Some(&serde_json::json!(OTHER_BASE)),
        "the participation keeps its own namespace beside the composer's default"
    );
    assert_eq!(
        performer.pointer("/id/value"),
        Some(&serde_json::json!("Practitioner/synthetic-practitioner-2"))
    );
    assert_eq!(
        performer.pointer("/id/scheme"),
        Some(&serde_json::json!("fhir-reference"))
    );
    Ok(())
}
