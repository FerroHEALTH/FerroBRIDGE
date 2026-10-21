// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A PROGRAMMED mapping, a manual mapping and a `hierarchy.split` inside a
//! `reference`, each climbing back out of the referenced resource with `^`
//! (`basics/path_operators.adoc`, §Recurrence and parent elements).

use core::error::Error;
use std::collections::BTreeMap;
use std::sync::Mutex;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::functions::Datum;
use fhirconnect::engine::traverse::functions::MappingCodeError;
use fhirconnect::engine::traverse::functions::MappingFunctions;
use fhirconnect::engine::traverse::functions::Ports;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::semantic::MappingCodeRegistry;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;

use crate::engine::MapIdentities;
use crate::engine::MapReferences;
use crate::engine::defaults;
use crate::support::compiled_models;
use crate::support::compiled_with;
use crate::support::template;

/// The function the fixture's PROGRAMMED mapping names.
const CARET_USE: &str = "caretUse";

/// A registry with one function, which writes the first code of the
/// `CodeableConcept` it is handed as the problem-diagnosis certainty.
#[derive(Debug)]
struct CaretUse;

impl MappingCodeRegistry for CaretUse {
    fn contains(&self, code: &str) -> bool {
        code == CARET_USE
    }

    fn ports(&self, code: &str) -> Option<&'static [&'static str]> {
        (code == CARET_USE).then_some(&["items[at0073]"])
    }
}

impl MappingFunctions for CaretUse {
    fn to_fhir(&self, _code: &str, _ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
        Ok(None)
    }

    fn to_openehr(&self, _code: &str, element: &Value) -> Result<Ports, MappingCodeError> {
        let Some(code) = element
            .get("coding")
            .and_then(|coding| coding.as_array()?.first())
            .and_then(|coding| coding.get("code"))
            .and_then(Value::as_str)
        else {
            return Ok(Ports::new());
        };
        Ok(Ports::new().with(
            "items[at0073]",
            Datum::new(
                "DV_CODED_TEXT",
                serde_json::json!({
                    "_type": "DV_CODED_TEXT",
                    "value": code,
                    "defining_code": {
                        "_type": "CODE_PHRASE",
                        "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "local"},
                        "code_string": code
                    }
                }),
            ),
        ))
    }
}

/// Returns a synthetic `Encounter` whose one diagnosis references the
/// synthetic `Condition` and carries `use_code` as its use.
fn encounter_using(use_code: &str) -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "Encounter",
        "id": "synthetic-encounter-1",
        "status": "finished",
        "class": {"system": "http://terminology.hl7.org/CodeSystem/v3-ActCode", "code": "AMB"},
        "diagnosis": [{
            "condition": {"reference": "Condition/synthetic-condition-1"},
            "use": {"coding": [{
                "system": "http://example.org/fhir/CodeSystem/ferrobridge-diagnosis-use",
                "code": use_code
            }]}
        }]
    }))
}

/// Returns the referenced `Condition` of [`encounter_using`].
fn referenced_condition() -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "Condition",
        "id": "synthetic-condition-1",
        "subject": {"reference": "Patient/synthetic-patient-1"},
        "code": {"text": "Synthetic problem one"}
    }))
}

/// Maps `encounter` through `fixture` with the referenced `Condition` held
/// by the reference seam and [`CaretUse`] as the registry.
fn run_in(
    fixture: &str,
    encounter: &Value,
) -> Result<(CanonicalComposition, WebTemplateIndex), Box<dyn Error>> {
    let index = template()?;
    let program = compiled_with(fixture, &index, &CaretUse)?;
    let mut held = BTreeMap::new();
    held.insert(
        String::from("Condition/synthetic-condition-1"),
        referenced_condition(),
    );
    let references = MapReferences(held);
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        encounter,
        &Seams::default()
            .with_references(&references)
            .with_functions(&CaretUse),
        &defaults(),
        &CallContext::new(),
    )?;
    Ok((inbound.into_value(), index))
}

/// Returns the code of the problem-diagnosis certainty a composition holds.
fn certainty_of(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<Option<String>, Box<dyn Error>> {
    let node = index.node(&AqlPath::new(
        "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0073]/value",
    ))?;
    Ok(index.read(composition, node, &[])?.and_then(|value| {
        value
            .get("defining_code")?
            .get("code_string")?
            .as_str()
            .map(String::from)
    }))
}

#[test]
fn a_programmed_mapping_reads_the_element_a_caret_climbs_to() -> Result<(), Box<dyn Error>> {
    // The function sees "the whole FHIR element" its `with.fhir` names
    // (concept-mappings.adoc §Programmed mappings), here the Encounter
    // diagnosis use reached from inside the referenced Condition.
    let (composition, index) = run_in(
        "ferrobridge_caret_kinds",
        &encounter_using("treatment-diagnosis"),
    )?;
    assert_eq!(
        certainty_of(&index, &composition)?.as_deref(),
        Some("treatment-diagnosis")
    );
    Ok(())
}

#[test]
fn a_manual_mapping_writes_the_element_a_caret_climbs_to() -> Result<(), Box<dyn Error>> {
    // manual.adoc: "full FHIR or openEHR elements are created from all given
    // paths"; the path climbs from the created Condition into the Encounter
    // diagnosis that references it.
    let (composition, index) = run_in(
        "ferrobridge_caret_kinds",
        &encounter_using("treatment-diagnosis"),
    )?;
    let program = compiled_with("ferrobridge_caret_kinds", &index, &CaretUse)?;
    let mut ids = BTreeMap::new();
    ids.insert(
        String::from("Condition"),
        String::from("created-condition-1"),
    );
    let identities = MapIdentities {
        ids,
        seen: Mutex::default(),
    };
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &Seams::default()
            .with_identities(&identities)
            .with_functions(&CaretUse),
        &CallContext::new(),
    )?;
    let diagnosis = outbound
        .value()
        .get("diagnosis")
        .and_then(|listed| listed.as_array()?.first())
        .ok_or("the Encounter carries its diagnosis")?;
    assert_eq!(
        diagnosis
            .get("use")
            .and_then(|used| used.get("text"))
            .and_then(Value::as_str),
        Some("Treatment diagnosis"),
        "{:?}",
        outbound.value()
    );
    let created = outbound
        .created()
        .first()
        .ok_or("the run created the Condition")?;
    assert!(
        created.get("use").is_none() && created.get("text").is_none(),
        "nothing the manual entry writes lands in the created Condition: {created:?}"
    );
    Ok(())
}

/// Returns the body-site names the anatomical-location clusters of a
/// composition carry, in order.
fn cluster_sites(composition: &CanonicalComposition) -> Vec<String> {
    composition
        .value()
        .pointer("/content/0/data/items")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("archetype_node_id")
                        .and_then(serde_json::Value::as_str)
                        == Some("openEHR-EHR-CLUSTER.anatomical_location.v1")
                })
                .filter_map(|cluster| {
                    cluster
                        .pointer("/items/0/value/value")
                        .and_then(serde_json::Value::as_str)
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_split_inside_a_reference_iterates_the_element_a_caret_climbs_to() -> Result<(), Box<dyn Error>>
{
    // HierarchyMappings.adoc §split: "for each occurrence" of the `with` path
    // an element is created; the path is the Encounter diagnosis use the slot
    // inside the referenced Condition climbed to.
    let index = template()?;
    let program = compiled_models(
        "ferrobridge_caret_split",
        &[
            "ferrobridge_caret_split",
            "ferrobridge_caret_split_location",
        ],
        &index,
        &CaretUse,
    )?;
    let mut held = BTreeMap::new();
    held.insert(
        String::from("Condition/synthetic-condition-1"),
        referenced_condition(),
    );
    let references = MapReferences(held);
    let encounter = Value::from_serde_json(serde_json::json!({
        "resourceType": "Encounter",
        "id": "synthetic-encounter-3",
        "status": "finished",
        "class": {"system": "http://terminology.hl7.org/CodeSystem/v3-ActCode", "code": "AMB"},
        "diagnosis": [{
            "condition": {"reference": "Condition/synthetic-condition-1"},
            "use": {"coding": [
                {"system": "http://example.org/fhir/CodeSystem/ferrobridge-diagnosis-use",
                 "code": "treatment-diagnosis", "display": "Treatment diagnosis"},
                {"system": "http://example.org/fhir/CodeSystem/ferrobridge-diagnosis-use",
                 "code": "referral-diagnosis", "display": "Referral diagnosis"}
            ]}
        }]
    }));
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &encounter,
        &Seams::default().with_references(&references),
        &defaults(),
        &CallContext::new(),
    )?;
    assert_eq!(
        cluster_sites(inbound.value()),
        ["Treatment diagnosis", "Referral diagnosis"]
    );
    Ok(())
}
