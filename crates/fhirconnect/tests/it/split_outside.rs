// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `hierarchy.split` in a file slotted below a repeating event, whose
//! split node lies outside that event, in both directions.
//!
//! `types-of-mappings/concept-type/HierarchyMappings.adoc` (§split) creates
//! one openEHR element per group of the split's FHIR path, and
//! `basics/path_operators.adoc` lets the path leave the slot by `../`, here
//! to the lots of the observation's protocol, which repeat with no event.
//! The synthetic template is `tests/fixtures/opt/observation_protocol.opt`;
//! the mappings are `ferrobridge_split_outside` and its slotted
//! `ferrobridge_split_outside_detail`.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::semantic::StaticMappingCodes;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;

use crate::laws;
use crate::leave_anchor::protocol_template;
use crate::support::compiled_models;

/// The lot of the protocol's batch.
const LOT: &str = "/content[openEHR-EHR-OBSERVATION.ferrobridge_reading.v1]/protocol[at0005]/items[at0009]/items[at0010]/value";

/// The context under test.
const CONTEXT: &str = "ferrobridge_split_outside";

/// A synthetic `Observation` of two readings, the first naming one lot and
/// the second another.
fn observation() -> Value {
    let component = |at: &str, note: &str, lot: &str| {
        serde_json::json!({
            "code": {"text": note, "coding": [{"display": lot}]},
            "valueDateTime": at
        })
    };
    Value::from_serde_json(serde_json::json!({
        "resourceType": "Observation",
        "status": "final",
        "code": {"text": "Synthetic reading"},
        "component": [
            component("2026-09-12T06:00:00+02:00", "Synthetic first note", "Synthetic lot A"),
            component("2026-09-12T07:00:00+02:00", "Synthetic second note", "Synthetic lot B")
        ]
    }))
}

/// Maps the synthetic `Observation` into a composition.
fn composition(index: &WebTemplateIndex) -> Result<CanonicalComposition, Box<dyn Error>> {
    let program = compiled_models(
        CONTEXT,
        &[CONTEXT, "ferrobridge_split_outside_detail"],
        index,
        &StaticMappingCodes::default(),
    )?;
    Ok(to_openehr(
        &program,
        &SCHEMAS,
        index,
        &observation(),
        &Seams::default(),
        &laws::defaults(),
        &CallContext::new(),
    )?
    .into_value())
}

/// Returns the text of the lot at `positions`, the batch's first.
fn lot(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    batch: u32,
    lot: u32,
) -> Result<Option<String>, Box<dyn Error>> {
    let node = index.node(&AqlPath::new(LOT))?;
    let positions = [RmPosition::new(batch)?, RmPosition::new(lot)?];
    Ok(index
        .read(composition, node, &positions)?
        .and_then(|value| value.get("value")?.as_str().map(String::from)))
}

#[test]
fn a_split_outside_a_repeating_anchor_creates_its_elements_in_the_one_batch()
-> Result<(), Box<dyn Error>> {
    // HierarchyMappings.adoc §split: each group creates its own element. The
    // batch around the lots repeats with no event, so the second reading's
    // lot joins the first batch instead of taking the event's instance.
    let index = protocol_template()?;
    let built = composition(&index)?;
    assert_eq!(
        (lot(&index, &built, 1, 1)?, lot(&index, &built, 1, 2)?),
        (
            Some(String::from("Synthetic lot A")),
            Some(String::from("Synthetic lot B"))
        ),
        "both lots are in the first batch"
    );
    assert_eq!(
        lot(&index, &built, 2, 1)?,
        None,
        "no batch is created for the second event"
    );
    Ok(())
}

#[test]
fn the_lots_outside_the_anchor_come_back_out_of_openehr() -> Result<(), Box<dyn Error>> {
    let index = protocol_template()?;
    let program = compiled_models(
        CONTEXT,
        &[CONTEXT, "ferrobridge_split_outside_detail"],
        &index,
        &StaticMappingCodes::default(),
    )?;
    let built = composition(&index)?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &built,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let json = outbound
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Observation"))?;
    let first: Vec<&str> = json
        .pointer("/component/0/code/coding")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("the first component carries its codings: {json}"))?
        .iter()
        .filter_map(|coding| coding.get("display")?.as_str())
        .collect();
    assert_eq!(
        first,
        ["Synthetic lot A", "Synthetic lot B"],
        "the lots repeat with no event, so each reading reads every one"
    );
    Ok(())
}
