// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A composition holding a context's start archetype more than once: one
//! resource per occurrence, each from its own subtree, with the context's
//! condition evaluated per occurrence (`to_fhir_each`), and the first-only
//! run that declares the occurrences it left (`to_fhir`).
//!
//! FHIRconnect v1.0.0 does not say how a context binds to a start archetype a
//! composition holds more than once, so the behaviour is the engine's own
//! design, after the per-occurrence runs of
//! `types-of-mappings/concept-type/HierarchyMappings.adoc`, §split.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::Each;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_fhir_each;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::load::load_set;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;

use crate::eu_condition::condition;
use crate::eu_condition::eu_condition_program;
use crate::laws;
use crate::roundtrip::kds_template;
use crate::support::FIXTURES;
use crate::support::render;

/// A synthetic Condition with `code` and, when given, a note.
fn diagnosis(
    code: &str,
    display: &str,
    note: Option<&str>,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut resource = condition();
    let object = resource
        .as_object_mut()
        .ok_or("the synthetic Condition is an object")?;
    object.insert(
        "code".to_owned(),
        serde_json::json!({"coding": [{
            "system": "http://snomed.info/sct",
            "code": code,
            "display": display
        }]}),
    );
    if let Some(note) = note {
        object.insert("note".to_owned(), serde_json::json!([{"text": note}]));
    }
    Ok(resource)
}

/// Returns one `KDS_Diagnose` composition holding two diagnoses: a
/// hypertension with a note and an asthma without one, in that order.
fn two_diagnoses(
    program: &Program,
    index: &WebTemplateIndex,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let built = |resource: serde_json::Value| {
        to_openehr(
            program,
            &SCHEMAS,
            index,
            &Value::from_serde_json(resource),
            &Seams::default(),
            &laws::defaults(),
            &CallContext::new(),
        )
        .map(fhirconnect::engine::outcome::Outcome::into_value)
    };
    let first = built(diagnosis(
        "38341003",
        "Hypertensive disorder, systemic arterial",
        Some("A synthetic first note"),
    )?)?;
    let second = built(diagnosis("195967001", "Asthma", None)?)?;
    let mut value = first.value().clone();
    let further = second
        .value()
        .get("content")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .ok_or("the second composition has content")?;
    value
        .get_mut("content")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("the first composition has content")?
        .extend(further);
    Ok(CanonicalComposition::new(
        value,
        first.template_id(),
        first.generation(),
    ))
}

/// The SNOMED CT code a Condition carries.
fn code_of(resource: &Value) -> Result<String, Box<dyn Error>> {
    let json = resource.to_serde_json(&mut fhir_types::codec::Path::root("Condition"))?;
    Ok(json
        .pointer("/code/coding/0/code")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("the Condition carries no code: {json}"))?
        .to_owned())
}

#[test]
fn each_occurrence_of_the_start_archetype_is_its_own_resource() -> Result<(), Box<dyn Error>> {
    let (program, index) = eu_condition_program()?;
    let composition = two_diagnoses(&program, &index)?;
    let each = to_fhir_each(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let mut mapped = Vec::new();
    for occurrence in &each {
        let Each::Mapped {
            occurrence,
            outcome,
        } = occurrence
        else {
            return Err(format!("every occurrence maps: {each:?}").into());
        };
        let json = outcome
            .value()
            .to_serde_json(&mut fhir_types::codec::Path::root("Condition"))?;
        mapped.push((occurrence.clone(), code_of(outcome.value())?, json));
    }
    let codes: Vec<(Vec<u32>, &str)> = mapped
        .iter()
        .map(|(occurrence, code, _)| (occurrence.clone(), code.as_str()))
        .collect();
    assert_eq!(
        codes,
        [(vec![1], "38341003"), (vec![2], "195967001")],
        "one Condition per occurrence, each with its own code"
    );
    assert_eq!(
        mapped
            .first()
            .and_then(|(_, _, json)| json.pointer("/note/0/text"))
            .and_then(serde_json::Value::as_str),
        Some("A synthetic first note"),
        "the first occurrence carries its own note"
    );
    assert!(
        mapped
            .get(1)
            .is_some_and(|(_, _, json)| json.get("note").is_none()),
        "the second occurrence carries no note of the first's: {mapped:?}"
    );
    Ok(())
}

#[test]
fn a_run_over_the_first_occurrence_declares_the_occurrences_it_left() -> Result<(), Box<dyn Error>>
{
    let (program, index) = eu_condition_program()?;
    let composition = two_diagnoses(&program, &index)?;
    let outcome = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    assert_eq!(
        code_of(outcome.value())?,
        "38341003",
        "the first occurrence"
    );
    let left: Vec<usize> = outcome
        .warnings()
        .iter()
        .filter_map(|warning| match *warning.loss() {
            Loss::UnmappedOccurrences { count, .. } => Some(count),
            _ => None,
        })
        .collect();
    assert_eq!(left, [1], "the run names the one occurrence it left");
    Ok(())
}

/// Compiles the context whose condition admits a diagnosis with a comment.
fn gated(index: &WebTemplateIndex) -> Result<std::sync::Arc<Program>, Box<dyn Error>> {
    let files: Vec<String> = [
        "ferrobridge_repeated_gate.context.yml",
        "ferrobridge_repeated_gate.yml",
    ]
    .iter()
    .map(|file| format!("{FIXTURES}/projects/ferrobridge/repeated_start/{file}"))
    .collect();
    let set = load_set(files, &StaticMappingCodes::default())
        .map_err(|diagnostics| render(&diagnostics))?;
    compile(
        &set,
        &MappingName::new("ferrobridge_repeated_gate.context")?,
        index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .map_err(|diagnostics| render(&diagnostics).into())
}

#[test]
fn the_contexts_condition_is_evaluated_per_occurrence() -> Result<(), Box<dyn Error>> {
    let (builder, index) = eu_condition_program()?;
    let composition = two_diagnoses(&builder, &kds_template()?)?;
    let program = gated(&index)?;
    let each = to_fhir_each(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let [
        Each::Mapped {
            occurrence: first,
            outcome,
        },
        Each::NotApplicable { occurrence: second },
    ] = each.as_slice()
    else {
        return Err(format!(
            "the first occurrence maps and the second is not applicable: {each:?}"
        )
        .into());
    };
    assert_eq!(
        (first.as_slice(), second.as_slice()),
        ([1].as_slice(), [2].as_slice())
    );
    let json = outcome
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Condition"))?;
    assert_eq!(
        json.pointer("/note/0/text")
            .and_then(serde_json::Value::as_str),
        Some("A synthetic first note"),
        "the admitted occurrence maps its own note: {json}"
    );
    Ok(())
}
