// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A demographic `Reference.reference` through the demographics endpoint on a
//! route whose FLAT family carries `PARTY_IDENTIFIED.identifiers`: the
//! `_provider` of an `ENTRY`.
//!
//! `PARTY_IDENTIFIED.adoc` resolves `reference` and adds "the resource
//! identifier" to `identifiers`; `engine/demographics.adoc` falls back to "the
//! IDs maintained by the FHIRconnect engine's FHIR-server".

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::demographics::IssuerDemographics;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;

use crate::support::compiled;
use crate::support::template;

/// The FHIR server base the bridge mints demographic identities under.
const BASE: &str = "https://bridge.example/fhir";

/// A synthetic `Condition` whose `recorder` is `recorder`.
fn condition(recorder: serde_json::Value) -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "Condition",
        "code": {
            "coding": [{
                "system": "http://example.org/fhir/CodeSystem/synthetic-problems",
                "code": "SYN-001",
                "display": "Synthetic problem one"
            }],
            "text": "Synthetic problem one"
        },
        "recorder": recorder
    }))
}

/// The defaults every run of this module takes.
fn defaults() -> Defaults {
    Defaults::at("2026-10-19T08:00:00Z")
        .with_language("en")
        .with_territory("NL")
}

#[test]
fn a_provider_reference_travels_as_an_identifier_and_reads_back_as_the_reference()
-> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_derived")?;
    let index = template()?;
    let demographics = IssuerDemographics::new(BASE)?;
    let seams = Seams::default().with_demographics(&demographics);
    let recorder = serde_json::json!({
        "reference": "Practitioner/synthetic-practitioner-1",
        "display": "Synthetic recorder"
    });
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(recorder.clone()),
        &seams,
        &defaults(),
        &CallContext::new(),
    )?;
    let flat = index.flatten(inbound.value())?;
    let value_of = |suffix: &str| {
        flat.iter()
            .find(|(key, _)| key.ends_with(suffix))
            .map(|(_, value)| value.clone())
    };
    assert_eq!(
        value_of("/_provider/_identifier:0|id"),
        Some(serde_json::json!("Practitioner/synthetic-practitioner-1"))
    );
    assert_eq!(
        value_of("/_provider/_identifier:0|issuer"),
        Some(serde_json::json!(BASE))
    );
    assert_eq!(
        value_of("/_provider/_identifier:0|type"),
        Some(serde_json::json!("fhir-reference"))
    );
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &seams,
        &CallContext::new(),
    )?;
    let back = outbound
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Condition"))?;
    assert_eq!(back["recorder"], recorder);
    Ok(())
}

#[test]
fn a_provider_reference_is_never_dropped_without_an_endpoint() -> Result<(), Box<dyn Error>> {
    // The reference was once left out of the composition without a trace; with no endpoint the
    // run refuses instead.
    let program = compiled("ferrobridge_derived")?;
    let index = template()?;
    let refused = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(serde_json::json!({"reference": "Practitioner/synthetic-practitioner-1"})),
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    );
    assert!(
        matches!(refused, Err(EngineError::Demographics { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_request_local_reference_is_refused_by_the_bridge_endpoint() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_derived")?;
    let index = template()?;
    let demographics = IssuerDemographics::new(BASE)?;
    let refused = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition(serde_json::json!({
            "reference": "urn:uuid:5b0e0c9c-0000-4000-8000-000000000001"
        })),
        &Seams::default().with_demographics(&demographics),
        &defaults(),
        &CallContext::new(),
    );
    assert!(
        matches!(refused, Err(EngineError::Demographics { .. })),
        "{refused:?}"
    );
    Ok(())
}
