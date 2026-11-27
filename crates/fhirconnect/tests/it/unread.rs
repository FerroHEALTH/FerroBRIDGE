// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The elements of a composition a run out to FHIR leaves unread, each
//! declared as `Loss::Unread` with its openEHR path, over FerroBRIDGE's
//! medication template and a composition generated from it with one
//! occurrence of every node (`openehr-sdt`'s example at the `Medium` detail
//! level). No specification governs the declaration: the engine's own
//! design.

use core::error::Error;
use std::sync::Arc;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::model::load::load_set;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::Generation;
use openehr_sdt::flat::example::{DetailLevel, apply_output_uid, example_composition};
use serde_json::Value;

use crate::eu_mpd::medication_template;
use crate::support::FIXTURES;
use crate::support::render;

/// Compiles the fixture context `name` against `index`.
fn program(index: &WebTemplateIndex, name: &str) -> Result<Arc<Program>, Box<dyn Error>> {
    let files: Vec<String> = [
        "ferrobridge_unread_action.context.yml",
        "ferrobridge_unread_action.yml",
        "ferrobridge_unread_bare.context.yml",
        "ferrobridge_unread_bare.yml",
        "ferrobridge_unread_order.context.yml",
        "ferrobridge_unread_order.yml",
    ]
    .iter()
    .map(|file| format!("{FIXTURES}/projects/ferrobridge/unread/{file}"))
    .collect();
    let set = load_set(files, &StaticMappingCodes::default())
        .map_err(|diagnostics| render(&diagnostics))?;
    Ok(compile(
        &set,
        &MappingName::new(name)?,
        index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .map_err(|diagnostics| render(&diagnostics))?)
}

/// The first object below `value` whose `archetype_node_id` is `id`, an
/// archetype id matched with or without the full version the example names.
fn entry<'a>(value: &'a mut Value, id: &str) -> Option<&'a mut Value> {
    let named = value.get("archetype_node_id").and_then(Value::as_str);
    if named.is_some_and(|named| {
        named == id
            || named
                .strip_prefix(id)
                .is_some_and(|rest| rest.starts_with('.'))
    }) {
        return Some(value);
    }
    match value {
        Value::Object(map) => map.values_mut().find_map(|child| entry(child, id)),
        Value::Array(items) => items.iter_mut().find_map(|child| entry(child, id)),
        _ => None,
    }
}

/// A composition of the medication template holding one occurrence of
/// every node, the medication order's activity given a timing, changed by
/// `edit`.
fn every_element(
    index: &WebTemplateIndex,
    edit: impl FnOnce(&mut Value) -> Result<(), Box<dyn Error>>,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let mut example = example_composition(index.web_template(), DetailLevel::Medium)?;
    apply_output_uid(&mut example, index.template_id());
    let order = entry(&mut example, "openEHR-EHR-INSTRUCTION.medication_order.v3")
        .ok_or("the example holds a medication order")?;
    let activity = order
        .get_mut("activities")
        .and_then(|activities| activities.get_mut(0))
        .and_then(Value::as_object_mut)
        .ok_or("the medication order holds an activity")?;
    activity.insert(
        String::from("timing"),
        serde_json::json!({
            "_type": "DV_PARSABLE",
            "value": "R/2026-11-27T08:00:00Z/P1D",
            "formalism": "ISO8601"
        }),
    );
    edit(&mut example)?;
    Ok(CanonicalComposition::new(
        example,
        index.template_id(),
        Generation::Adl2,
    ))
}

/// The node and openEHR path of each element the run of `name` over
/// `composition` declares unread.
fn unread(
    index: &WebTemplateIndex,
    name: &str,
    composition: &CanonicalComposition,
) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let outcome = to_fhir(
        &*program(index, name)?,
        &SCHEMAS,
        index,
        composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    Ok(outcome
        .warnings()
        .iter()
        .filter_map(|warning| match *warning.loss() {
            Loss::Unread { ref node } => Some((
                node.clone(),
                warning.location().openehr().unwrap_or_default().to_owned(),
            )),
            _ => None,
        })
        .collect())
}

/// The nodes of `declared`.
fn nodes(declared: &[(String, String)]) -> Vec<&str> {
    declared.iter().map(|(node, _)| node.as_str()).collect()
}

#[test]
fn an_element_no_mapping_reads_is_declared_by_its_path() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let declared = unread(
        &index,
        "ferrobridge_unread_action.context",
        &every_element(&index, |_| Ok(()))?,
    )?;
    let (_, reason) = declared
        .iter()
        .find(|(node, _)| node == "at0021")
        .ok_or(format!("the reason is declared: {declared:?}"))?;
    assert!(
        reason.ends_with(
            "[openEHR-EHR-ACTION.medication.v1]/description[at0017]/items[at0021]/value"
        ),
        "{reason}"
    );
    for (node, path) in &declared {
        assert!(
            path.contains(&format!("[{node}]")) || path.ends_with(node),
            "{node} is named by its path: {path}"
        );
    }
    Ok(())
}

#[test]
fn a_read_element_is_not_declared() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let declared = unread(
        &index,
        "ferrobridge_unread_action.context",
        &every_element(&index, |_| Ok(()))?,
    )?;
    assert!(!nodes(&declared).contains(&"at0020"), "{declared:?}");
    Ok(())
}

#[test]
fn an_element_a_manual_condition_reads_is_not_declared() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let declared = unread(
        &index,
        "ferrobridge_unread_action.context",
        &every_element(&index, |_| Ok(()))?,
    )?;
    assert!(
        !nodes(&declared).contains(&"at0024"),
        "the comment: {declared:?}"
    );
    let bare = unread(
        &index,
        "ferrobridge_unread_bare.context",
        &every_element(&index, |_| Ok(()))?,
    )?;
    assert!(nodes(&bare).contains(&"at0024"), "the comment: {bare:?}");
    Ok(())
}

#[test]
fn an_element_of_two_data_value_types_is_declared_unless_read() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let composition = every_element(&index, |_| Ok(()))?;
    let bare = unread(&index, "ferrobridge_unread_bare.context", &composition)?;
    let (_, order_id) = bare
        .iter()
        .find(|(node, _)| node == "at0103")
        .ok_or(format!("the order ID is declared: {bare:?}"))?;
    assert!(
        order_id.contains("[openEHR-EHR-ACTION.medication.v1]/protocol[at0030]/items[at0103]"),
        "{order_id}"
    );
    let read = unread(&index, "ferrobridge_unread_action.context", &composition)?;
    assert!(
        !nodes(&read).contains(&"at0103"),
        "the order ID is read: {read:?}"
    );
    Ok(())
}

#[test]
fn an_element_of_two_data_value_types_held_as_its_second_type_is_declared()
-> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let composition = every_element(&index, |example| {
        let action = entry(example, "openEHR-EHR-ACTION.medication.v1")
            .ok_or("the example holds a medication action")?;
        let order_id = entry(&mut action["protocol"], "at0103").ok_or("the order ID")?;
        order_id["value"] = serde_json::json!({ "_type": "DV_TEXT", "value": "synthetic-order-1" });
        Ok(())
    })?;
    let bare = unread(&index, "ferrobridge_unread_bare.context", &composition)?;
    assert!(nodes(&bare).contains(&"at0103"), "{bare:?}");
    Ok(())
}

#[test]
fn an_activity_timing_no_mapping_reads_is_declared() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let declared = unread(
        &index,
        "ferrobridge_unread_order.context",
        &every_element(&index, |_| Ok(()))?,
    )?;
    let (_, timing) = declared
        .iter()
        .find(|(node, _)| node == "timing")
        .ok_or(format!("the timing is declared: {declared:?}"))?;
    assert!(timing.ends_with("/activities[at0001]/timing"), "{timing}");
    Ok(())
}
