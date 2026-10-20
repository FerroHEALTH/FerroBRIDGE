// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An `OBSERVATION` whose `HISTORY` carries events, each with an
//! `ITEM_LIST`, in both directions and under both lens laws.
//!
//! `EVENT.adoc` maps a FHIR `dateTime` to a `POINT_EVENT` and transforms
//! recurring events "by `HierarchyMappings.adoc`": going out of openEHR each
//! event becomes its own resource (§split). The synthetic template is
//! `tests/fixtures/opt/observation_events.opt`; the mapping is
//! `ferrobridge_events`, which carries the history's origin and each event's
//! time as the start and the end of `effectivePeriod`, so the offset of an
//! event from the origin travels both ways.

use core::error::Error;
use std::sync::Arc;

use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::ast::ContextMappingFile;
use fhirconnect::model::ast::ModelMappingFile;
use fhirconnect::model::load::MappingSet;
use fhirconnect::model::parse::lower_context;
use fhirconnect::model::parse::lower_model;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::composition::NodeValue;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::template::TemplateSource;
use openehr_sdt::flat::path::Segment;

use crate::laws;
use crate::support::FIXTURES;
use crate::support::render;

/// The synthetic observation.
const OBSERVATION: &str = "/content[openEHR-EHR-OBSERVATION.ferrobridge_reading.v1]";

/// The history of the synthetic observation.
const HISTORY: &str = "/content[openEHR-EHR-OBSERVATION.ferrobridge_reading.v1]/data[at0001]";

/// The origin of the synthetic history.
const ORIGIN: &str = "2026-09-12T06:00:00+02:00";

/// Builds the index over the synthetic events template.
pub(crate) fn events_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let xml = std::fs::read_to_string(format!("{FIXTURES}/opt/observation_events.opt"))?;
    let opt = openehr_its::opt14::from_xml(&xml)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Compiles the events context and model against the events template.
pub(crate) fn events_program(index: &WebTemplateIndex) -> Result<Arc<Program>, Box<dyn Error>> {
    let mut set = MappingSet::new();
    for name in [
        "contexts/ferrobridge_events.context.yml",
        "model/ferrobridge_events.yml",
    ] {
        let text = std::fs::read_to_string(format!("{FIXTURES}/{name}"))?;
        let document = openehr_mapping_core::loader::load_str(name, &text)?;
        if text.contains("\ntype: context\n") {
            let file: ContextMappingFile =
                lower_context(&document).map_err(|diagnostics| render(&diagnostics))?;
            set.insert_context(file)
                .map_err(|error| error.to_string())?;
        } else {
            let file: ModelMappingFile =
                lower_model(&document).map_err(|diagnostics| render(&diagnostics))?;
            set.insert_model(file).map_err(|error| error.to_string())?;
        }
    }
    let context = MappingName::new("ferrobridge_events.context")?;
    compile(
        &set,
        &context,
        index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .map_err(|diagnostics| Box::<dyn Error>::from(render(&diagnostics)))
}

/// Returns a synthetic `Observation` of one reading taken `at`.
pub(crate) fn reading(at: &str, amount: u32) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Observation",
        "status": "final",
        "code": {"text": "Synthetic reading"},
        "effectivePeriod": {"start": ORIGIN, "end": at},
        "valueQuantity": {
            "value": amount,
            "unit": "kg",
            "system": "http://unitsofmeasure.org",
            "code": "kg"
        }
    })
}

/// Builds a composition of the events template: the origin and one event
/// per `(time, amount)` pair, in order.
pub(crate) fn readings(
    index: &WebTemplateIndex,
    events: &[(&str, u32)],
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let mut values = vec![
        NodeValue::new(
            index.node(&AqlPath::new("/composer"))?,
            "FerroBRIDGE".into(),
        )
        .with_datum("name"),
        NodeValue::new(index.node(&AqlPath::new("/language"))?, "de".into()).with_datum("code"),
        NodeValue::new(index.node(&AqlPath::new("/territory"))?, "DE".into()).with_datum("code"),
        NodeValue::new(index.node(&AqlPath::new(OBSERVATION))?, ORIGIN.into()).under(vec![
            Segment {
                name: String::from("history_origin"),
                index: None,
            },
        ]),
    ];
    for (instance, &(time, amount)) in (1u32..).zip(events) {
        let position = vec![RmPosition::new(instance)?];
        values.push(
            NodeValue::new(
                index.node(&AqlPath::new(format!("{HISTORY}/events[at0002]/time")))?,
                time.into(),
            )
            .with_occurrences(position.clone()),
        );
        values.push(
            NodeValue::new(
                index.node(&AqlPath::new(format!(
                    "{HISTORY}/events[at0002]/data[at0003]/items[at0004]/value"
                )))?,
                serde_json::json!({
                    "_type": "DV_QUANTITY",
                    "magnitude": f64::from(amount),
                    "units": "kg",
                    "precision": 0
                }),
            )
            .with_occurrences(position)
            .with_datum("raw"),
        );
    }
    Ok(index.build_composition(&values, laws::NOW)?)
}

/// Returns the FLAT entries of the observation, leaving out the composition
/// fields the engine defaults.
fn observation_keys(
    flat: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    flat.into_iter()
        .filter(|(key, _)| key.starts_with("synthetic_observation_events/synthetic_reading"))
        .collect()
}

#[test]
fn the_events_context_compiles_against_the_synthetic_template() -> Result<(), Box<dyn Error>> {
    let index = events_template()?;
    let program = events_program(&index)?;
    assert_eq!(program.resource().as_str(), "Observation");
    Ok(())
}

#[test]
fn putget_holds_on_the_observation_events_chain() -> Result<(), Box<dyn Error>> {
    let index = events_template()?;
    let program = events_program(&index)?;
    let declared = laws::putget(
        &program,
        &index,
        &reading("2026-09-12T08:30:00+02:00", 70),
        &laws::untouched,
    )?;
    insta::assert_json_snapshot!("observation_events_putget", declared);
    Ok(())
}

#[test]
fn getput_holds_on_the_observation_events_chain() -> Result<(), Box<dyn Error>> {
    let index = events_template()?;
    let program = events_program(&index)?;
    let declared = laws::getput(
        &program,
        &index,
        &readings(&index, &[("2026-09-12T08:30:00+02:00", 70)])?,
        &laws::untouched,
    )?;
    insta::assert_json_snapshot!("observation_events_getput", declared);
    Ok(())
}

#[test]
fn each_event_becomes_its_own_observation_and_maps_back_with_its_offset()
-> Result<(), Box<dyn Error>> {
    // HierarchyMappings.adoc §split: "for each event in openEHR, one
    // `resource` must be created".
    let index = events_template()?;
    let program = events_program(&index)?;
    let events = [
        ("2026-09-12T08:30:00+02:00", 70),
        ("2026-09-12T20:15:00+02:00", 71),
    ];
    let composition = readings(&index, &events)?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let resources: Vec<&Value> = core::iter::once(outbound.value())
        .chain(outbound.created())
        .collect();
    assert_eq!(resources.len(), events.len(), "one resource per event");
    for (resource, &(time, amount)) in resources.into_iter().zip(&events) {
        let json = resource.to_serde_json(&mut Path::root("Observation"))?;
        assert_eq!(
            json.pointer("/effectivePeriod/start"),
            Some(&serde_json::json!(ORIGIN)),
            "every resource keeps the history's origin: {json}"
        );
        assert_eq!(
            json.pointer("/effectivePeriod/end"),
            Some(&serde_json::json!(time)),
            "{json}"
        );
        assert_eq!(
            json.pointer("/valueQuantity/value"),
            Some(&serde_json::json!(amount)),
            "{json}"
        );
        let back = to_openehr(
            &program,
            &SCHEMAS,
            &index,
            resource,
            &Seams::default(),
            &laws::defaults(),
            &CallContext::new(),
        )?;
        let single = readings(&index, &[(time, amount)])?;
        assert_eq!(
            observation_keys(index.flatten(back.value())?),
            observation_keys(index.flatten(&single)?),
            "the resource of the event at {time} maps back into that one event"
        );
    }
    Ok(())
}
