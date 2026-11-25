// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A source's logical models generated from its `OpenAPI` schemas, and a map
//! over a synthetic client and BSN record that renders an R4 `Patient`.

use std::error::Error;

use ferrobridge_server::structuremap::engine::Engine;
use ferrobridge_server::structuremap::model::{Known, Structures};
use ferrobridge_server::structuremap::openapi::{OpenApiError, logical_model};
use ferrobridge_server::structuremap::parse::parse;
use fhir_types::codec::Value;
use serde_json::json;

/// The directory of the fixtures.
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/it/structuremap/fixtures"
);

/// Reads the fixture `name`.
fn fixture(name: &str) -> Result<String, Box<dyn Error>> {
    Ok(std::fs::read_to_string(format!("{FIXTURES}/{name}"))?)
}

/// The structures of the Client and Bsn models.
fn structures() -> Result<Known, Box<dyn Error>> {
    let document: Value = serde_json::from_str(&fixture("ons-openapi.json")?)?;
    let mut known = Known::default();
    for name in ["Client", "Bsn"] {
        let url = format!("http://ferrobridge.example/StructureDefinition/{name}");
        known.logical.add(&logical_model(&document, name, &url)?)?;
    }
    Ok(known)
}

/// Returns `value` as a codec value.
fn codec(value: serde_json::Value) -> Value {
    Value::from_serde_json(value)
}

#[test]
fn the_schemas_become_logical_models_with_their_inherited_and_nested_elements()
-> Result<(), Box<dyn Error>> {
    let known = structures()?;
    let first = known
        .element("Client", "firstName")
        .ok_or("firstName from Person")?;
    assert_eq!(Some("string"), first.type_name.as_deref());
    let born = known
        .element("Client", "dateOfBirth")
        .ok_or("dateOfBirth")?;
    assert_eq!(Some("date"), born.type_name.as_deref());
    let died = known.element("Client", "deathDate").ok_or("deathDate")?;
    assert_eq!(Some("dateTime"), died.type_name.as_deref());
    let allocations = known
        .element("Client", "careAllocations")
        .ok_or("careAllocations")?;
    assert!(allocations.many);
    assert_eq!(
        Some("Client.careAllocations"),
        allocations.type_name.as_deref()
    );
    assert!(
        known
            .element("Client.careAllocations", "beginDate")
            .is_some()
    );
    assert_eq!(None, known.element("Client", "number"));
    let document: Value = serde_json::from_str(&fixture("ons-openapi.json")?)?;
    assert_eq!(
        Err(OpenApiError::NoSchema(String::from("Employee"))),
        logical_model(&document, "Employee", "urn:x")
    );
    Ok(())
}

#[test]
fn a_client_and_its_bsn_render_a_patient() -> Result<(), Box<dyn Error>> {
    let map = parse(&fixture("ons-patient.map")?)?;
    let engine = Engine::new(vec![map], structures()?);
    let client = codec(json!({
        "id": 4711,
        "firstName": "Testa",
        "lastName": "Voorbeeld",
        "dateOfBirth": "1950-02-03",
        "gender": "F",
        "deathDate": "2024-05-06T07:08:09+02:00"
    }));
    let bsn = codec(json!({"clientId": 4711, "number": "999990019", "idVerified": true}));
    let output = engine.transform(
        "http://ferrobridge.example/StructureMap/client-patient",
        &[client, bsn],
        "Patient",
    )?;
    let patient = serde_json::to_value(&output.target)?;
    assert_eq!(
        json!({
            "resourceType": "Patient",
            "identifier": [
                {"system": "urn:example:tenant:t1", "value": "4711"},
                {"system": "http://fhir.nl/fhir/NamingSystem/bsn", "value": "999990019"}
            ],
            "name": [{"family": "Voorbeeld", "given": ["Testa"]}],
            "birthDate": "1950-02-03",
            "gender": "female",
            "deceasedDateTime": "2024-05-06T07:08:09+02:00"
        }),
        patient
    );
    Ok(())
}

#[test]
fn an_unverified_bsn_and_absent_fields_are_left_out() -> Result<(), Box<dyn Error>> {
    let map = parse(&fixture("ons-patient.map")?)?;
    let engine = Engine::new(vec![map], structures()?);
    let client = codec(json!({"id": 12, "lastName": "Voorbeeld", "gender": "U"}));
    let bsn = codec(json!({"number": "999990019", "idVerified": false}));
    let output = engine.transform(
        "http://ferrobridge.example/StructureMap/client-patient",
        &[client, bsn],
        "Patient",
    )?;
    assert_eq!(
        json!({
            "resourceType": "Patient",
            "identifier": [{"system": "urn:example:tenant:t1", "value": "12"}],
            "name": [{"family": "Voorbeeld"}],
            "gender": "unknown"
        }),
        serde_json::to_value(&output.target)?
    );
    Ok(())
}
