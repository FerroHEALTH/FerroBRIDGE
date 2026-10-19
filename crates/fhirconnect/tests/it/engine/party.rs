// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A party program through the engine: a `Patient` into a `PERSON` of the
//! testkit's synthetic party template and back. No specification governs a
//! party context: our own design.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::party::from_party;
use fhirconnect::engine::traverse::party::to_party;
use openehr_mapping_core::party::PartyClass;
use openehr_rm::v1_2::demographic::person::Person;

use crate::resolve::party::NAME;
use crate::resolve::party::party_program;
use crate::resolve::party::person;
use crate::resolve::party::person_context;
use crate::resolve::party::person_model;
use crate::resolve::render;
use crate::resolve::set_of;

/// The mapping onto the identifier element of the details tree.
const IDENTIFIER: &str = "  - name: \"identifier\"\n    with:\n      fhir: \"$resource.identifier\"\n      \
                          openehr: \"$archetype/details[at0001]/items[at0002]\"\n";

/// Compiles the party context over the name and identifier mappings.
fn program() -> Result<std::sync::Arc<fhirconnect::resolve::program::Program>, Box<dyn Error>> {
    let model = person_model("PERSON.synthetic.v1", &format!("{NAME}{IDENTIFIER}"));
    let context = person_context(&[]);
    let set = set_of(&[("model.yml", &model), ("context.yml", &context)])?;
    Ok(party_program(&set, "person.context").map_err(|diagnostics| render(&diagnostics))?)
}

/// A synthetic `Patient` with a name and an identifier.
fn patient() -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "Patient",
        "identifier": [{"system": "http://example.org/fhir/sid/synthetic", "value": "SYN-0001"}],
        "name": [{"text": "Synthetic Person"}]
    }))
}

#[test]
fn a_patient_maps_into_a_person() -> Result<(), Box<dyn Error>> {
    let program = program()?;
    let index = person()?;
    let outcome = to_party(
        &program,
        &SCHEMAS,
        &index,
        &patient(),
        &Seams::default(),
        &CallContext::new(),
    )?;
    let party = outcome.into_value();
    assert_eq!(party.class(), PartyClass::Person);
    let person: Person = openehr_its::json::from_canonical_value(party.value())?;
    assert_eq!(person.identities.len(), 1);
    assert_eq!(
        party
            .value()
            .pointer("/identities/0/details/items/0/value/value"),
        Some(&serde_json::Value::from("Synthetic Person"))
    );
    assert_eq!(
        party.value().pointer("/details/items/0/value/id"),
        Some(&serde_json::Value::from("SYN-0001"))
    );
    Ok(())
}

#[test]
fn a_person_maps_back_into_the_same_patient() -> Result<(), Box<dyn Error>> {
    let program = program()?;
    let index = person()?;
    let party = to_party(
        &program,
        &SCHEMAS,
        &index,
        &patient(),
        &Seams::default(),
        &CallContext::new(),
    )?
    .into_value();
    let back = from_party(
        &program,
        &SCHEMAS,
        &index,
        &party,
        &Seams::default(),
        &CallContext::new(),
    )?
    .into_value();
    assert_eq!(back, patient());
    Ok(())
}

#[test]
fn a_composition_program_never_reaches_the_party_build() -> Result<(), Box<dyn Error>> {
    let program = crate::support::compiled("ferrobridge_composer")?;
    let refused = to_party(
        &program,
        &SCHEMAS,
        &person()?,
        &patient(),
        &Seams::default(),
        &CallContext::new(),
    );
    assert!(
        matches!(refused, Err(EngineError::CompositionProgram { .. })),
        "a composition program reached the party build: {refused:?}"
    );
    Ok(())
}
