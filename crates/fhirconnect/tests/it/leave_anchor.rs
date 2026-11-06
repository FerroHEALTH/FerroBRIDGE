// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `followedBy` anchored at a repeating node whose mappings leave the
//! anchor for a node outside it, in both directions.
//!
//! `basics/path_operators.adoc` defines `../` as the parent of the anchor
//! ("It simply goes back one element, accessing the parent"), and
//! `basics/Variables.adoc` makes `$archetype` the archetype root, so a path
//! below the event can name the observation's protocol, which lies outside
//! every event and repeats with none of them. The synthetic template is
//! `tests/fixtures/opt/observation_protocol.opt`; the mapping is
//! one of `ferrobridge_leave_anchor_parent`, `_archetype` and `_reference`,
//! which reach the protocol by the parent step, by an absolute `$archetype`
//! path and inside a `reference`, one form each.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Outcome;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::template::TemplateSource;

use crate::laws;
use crate::support::FIXTURES;
use crate::support::compiled_against;

/// The synthetic observation.
const OBSERVATION: &str = "/content[openEHR-EHR-OBSERVATION.ferrobridge_reading.v1]";

/// Builds the index over the synthetic protocol template.
pub(crate) fn protocol_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let xml = std::fs::read_to_string(format!("{FIXTURES}/opt/observation_protocol.opt"))?;
    let opt = openehr_its::opt14::from_xml(&xml)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// A synthetic `Observation` carrying a method, a body site and a contained
/// specimen, each of which one of the mappings writes outside the event.
fn observation() -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "Observation",
        "status": "final",
        "code": {"text": "Synthetic reading"},
        "contained": [{
            "resourceType": "Specimen",
            "id": "sample",
            "type": {"text": "Synthetic sample"}
        }],
        "effectiveDateTime": "2026-09-12T06:00:00+02:00",
        "valueQuantity": {
            "value": 12,
            "unit": "kg",
            "system": "http://unitsofmeasure.org",
            "code": "kg"
        },
        "method": {"text": "Synthetic method"},
        "bodySite": {"text": "Synthetic site"},
        "specimen": {"reference": "#sample"}
    }))
}

/// Maps the synthetic `Observation` into a composition through `fixture`.
fn composition(
    index: &WebTemplateIndex,
    fixture: &str,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let program = compiled_against(fixture, index)?;
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

/// Maps the synthetic `Observation` into a composition through `fixture`
/// and back out of it.
fn round_trip(fixture: &str) -> Result<Outcome<Value>, Box<dyn Error>> {
    let index = protocol_template()?;
    let program = compiled_against(fixture, &index)?;
    let built = composition(&index, fixture)?;
    Ok(to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &built,
        &Seams::default(),
        &CallContext::new(),
    )?)
}

/// Returns the text the composition holds at the protocol item `at`, which
/// repeats with no event, so it is read with no occurrence.
fn protocol_text(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    at: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    let node = index.node(&AqlPath::new(format!(
        "{OBSERVATION}/protocol[at0005]/items[{at}]/value"
    )))?;
    Ok(index
        .read(composition, node, &[])?
        .and_then(|value| value.get("value")?.as_str().map(String::from)))
}

/// Returns the `text` of the `CodeableConcept` a resource holds at `member`.
fn text_of(resource: &Value, member: &str) -> Option<String> {
    resource
        .get(member)?
        .get("text")
        .and_then(Value::as_str)
        .map(String::from)
}

#[test]
fn a_parent_step_out_of_a_repeating_anchor_writes_the_node_it_names() -> Result<(), Box<dyn Error>>
{
    // path_operators.adoc: `../` "goes back one element, accessing the
    // parent", so `../../protocol[at0005]` below the event is the protocol.
    let index = protocol_template()?;
    let built = composition(&index, "ferrobridge_leave_anchor_parent")?;
    assert_eq!(
        protocol_text(&index, &built, "at0006")?.as_deref(),
        Some("Synthetic method")
    );
    Ok(())
}

#[test]
fn a_parent_step_out_of_a_repeating_anchor_reads_the_node_it_names() -> Result<(), Box<dyn Error>> {
    let outbound = round_trip("ferrobridge_leave_anchor_parent")?;
    assert_eq!(
        text_of(outbound.value(), "method").as_deref(),
        Some("Synthetic method")
    );
    Ok(())
}

#[test]
fn an_archetype_path_out_of_a_repeating_anchor_writes_the_node_it_names()
-> Result<(), Box<dyn Error>> {
    // Variables.adoc: `$archetype` is the archetype root wherever it is used.
    let index = protocol_template()?;
    let built = composition(&index, "ferrobridge_leave_anchor_archetype")?;
    assert_eq!(
        protocol_text(&index, &built, "at0007")?.as_deref(),
        Some("Synthetic site")
    );
    Ok(())
}

#[test]
fn an_archetype_path_out_of_a_repeating_anchor_reads_the_node_it_names()
-> Result<(), Box<dyn Error>> {
    let outbound = round_trip("ferrobridge_leave_anchor_archetype")?;
    assert_eq!(
        text_of(outbound.value(), "bodySite").as_deref(),
        Some("Synthetic site")
    );
    Ok(())
}

#[test]
fn a_reference_below_a_repeating_anchor_writes_outside_it() -> Result<(), Box<dyn Error>> {
    // Reference.adoc: the referenced resource's mappings run over it; their
    // `$archetype` stays the archetype root of the mapping around them.
    let index = protocol_template()?;
    let built = composition(&index, "ferrobridge_leave_anchor_reference")?;
    assert_eq!(
        protocol_text(&index, &built, "at0008")?.as_deref(),
        Some("Synthetic sample")
    );
    Ok(())
}

#[test]
fn a_reference_below_a_repeating_anchor_reads_outside_it() -> Result<(), Box<dyn Error>> {
    let outbound = round_trip("ferrobridge_leave_anchor_reference")?;
    let specimen = outbound
        .created()
        .first()
        .ok_or("the run created the Specimen")?;
    assert_eq!(
        text_of(specimen, "type").as_deref(),
        Some("Synthetic sample")
    );
    Ok(())
}

#[test]
fn the_event_keeps_its_own_instance_beside_the_protocol() -> Result<(), Box<dyn Error>> {
    // The event's own values stay at the instance the anchor bound.
    let index = protocol_template()?;
    let built = composition(&index, "ferrobridge_leave_anchor_parent")?;
    let amount = index.node(&AqlPath::new(format!(
        "{OBSERVATION}/data[at0001]/events[at0002]/data[at0003]/items[at0004]/value"
    )))?;
    let read = index.read(&built, amount, &[RmPosition::new(1)?])?;
    assert_eq!(
        read.as_ref()
            .and_then(|value| value.get("magnitude"))
            .and_then(serde_json::Value::as_f64),
        Some(12.0)
    );
    Ok(())
}
