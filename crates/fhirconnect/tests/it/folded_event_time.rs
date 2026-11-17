// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FHIR time mapped onto a single `EVENT` the Web Template collapses into
//! its `OBSERVATION`.
//!
//! The template keeps the event's mandatory `time` leaf at the event's path
//! and no node of the event itself (Simplified Formats master04
//! §"Conditionally Collapsed Wrapper Types"). `EVENT.adoc` §`POINT_EVENT` and
//! `INTERVAL_EVENT` still pairs a `dateTime` with a `POINT_EVENT` and a
//! `Period` with an `INTERVAL_EVENT`, so the mapping writes and reads the
//! event through that leaf and the object that holds it. The template is
//! `tests/fixtures/opt/observation_single_event.opt`, the events template with
//! at most one event; the mapping is `ferrobridge_event_time`, the form the
//! published `medication_statement.v0.yml` writes.

use core::error::Error;

use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::SkipReason;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::template::TemplateSource;
use serde_json::json;
use std::sync::Arc;

use crate::laws;
use crate::support::FIXTURES;
use crate::support::compiled_models;

/// The `time` leaf the template keeps for the collapsed event.
const TIME: &str =
    "/content[openEHR-EHR-OBSERVATION.ferrobridge_reading.v1]/data[at0001]/events[at0002]/time";

/// Builds the index over the single-event template.
fn single_event_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let xml = std::fs::read_to_string(format!("{FIXTURES}/opt/observation_single_event.opt"))?;
    let opt = openehr_its::opt14::from_xml(&xml)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Compiles the event time mapping against the single-event template.
fn program(index: &WebTemplateIndex) -> Result<Arc<Program>, Box<dyn Error>> {
    compiled_models(
        "ferrobridge_single_event_time",
        &["ferrobridge_event_time"],
        index,
        &StaticMappingCodes::default(),
    )
}

/// Returns a synthetic `Observation` whose `effective[x]` is `key: value`.
fn observation(key: &str, value: serde_json::Value) -> serde_json::Value {
    let mut resource = json!({
        "resourceType": "Observation",
        "status": "final",
        "code": {"text": "Synthetic reading"},
        "valueQuantity": {
            "value": 70,
            "unit": "kg",
            "system": "http://unitsofmeasure.org",
            "code": "kg"
        }
    });
    if let Some(object) = resource.as_object_mut() {
        object.insert(String::from(key), value);
    }
    resource
}

/// Runs `resource` into openEHR.
fn inbound(
    index: &WebTemplateIndex,
    resource: &serde_json::Value,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    Ok(to_openehr(
        &*program(index)?,
        &SCHEMAS,
        index,
        &Value::from_serde_json(resource.clone()),
        &Seams::default(),
        &laws::defaults(),
        &CallContext::new(),
    )?
    .into_value())
}

/// Returns the event the composition holds, the object that holds its
/// `time` leaf.
fn event_of(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let node = index.node(&AqlPath::new(TIME))?;
    Ok(index
        .read_holder(composition, node, &[])?
        .ok_or("the composition holds the event")?)
}

/// Runs `composition` out to FHIR and returns the resource with the losses.
fn outbound(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<(serde_json::Value, Vec<Loss>), Box<dyn Error>> {
    let out = to_fhir(
        &*program(index)?,
        &SCHEMAS,
        index,
        composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let losses = out
        .warnings()
        .iter()
        .map(|warning| warning.loss().clone())
        .collect();
    Ok((
        out.value().to_serde_json(&mut Path::root("Observation"))?,
        losses,
    ))
}

#[test]
fn the_template_folds_the_event_into_its_time_leaf() -> Result<(), Box<dyn Error>> {
    let index = single_event_template()?;
    let event =
        "/content[openEHR-EHR-OBSERVATION.ferrobridge_reading.v1]/data[at0001]/events[at0002]";
    assert!(index.node(&AqlPath::new(event)).is_err(), "no event node");
    assert_eq!(index.node(&AqlPath::new(TIME))?.rm_type(), "DV_DATE_TIME");
    Ok(())
}

#[test]
fn the_mapping_derives_a_folded_event_time_onto_the_time_leaf() -> Result<(), Box<dyn Error>> {
    let rendered = program(&single_event_template()?)?.to_string();
    assert!(
        rendered.contains("event time read")
            && rendered.contains("events[at0002]/time")
            && rendered.contains(" folded")
            && rendered.contains("as(DateTime)")
            && rendered.contains("as(Period)"),
        "{rendered}"
    );
    Ok(())
}

#[test]
fn a_date_time_goes_in_as_a_point_event_and_comes_back() -> Result<(), Box<dyn Error>> {
    let index = single_event_template()?;
    let composition = inbound(
        &index,
        &observation("effectiveDateTime", json!("2026-09-12T08:30:00+02:00")),
    )?;
    let event = event_of(&index, &composition)?;
    assert_eq!(event.get("_type"), Some(&json!("POINT_EVENT")));
    assert_eq!(
        event.pointer("/time/value"),
        Some(&json!("2026-09-12T08:30:00+02:00"))
    );
    let (resource, _) = outbound(&index, &composition)?;
    assert_eq!(
        resource.get("effectiveDateTime"),
        Some(&json!("2026-09-12T08:30:00+02:00")),
        "{resource}"
    );
    assert_eq!(resource.get("effectivePeriod"), None);
    Ok(())
}

#[test]
fn a_period_goes_in_as_an_interval_event_and_comes_back() -> Result<(), Box<dyn Error>> {
    let index = single_event_template()?;
    let period = json!({"start": "2026-09-12T06:00:00+02:00", "end": "2026-09-12T08:30:00+02:00"});
    let composition = inbound(&index, &observation("effectivePeriod", period.clone()))?;
    let event = event_of(&index, &composition)?;
    assert_eq!(event.get("_type"), Some(&json!("INTERVAL_EVENT")));
    assert_eq!(
        event.pointer("/time/value"),
        Some(&json!("2026-09-12T08:30:00+02:00")),
        "the time is the trailing edge"
    );
    assert_eq!(event.pointer("/width/value"), Some(&json!("PT2H30M")));
    assert_eq!(
        event.pointer("/math_function/defining_code/code_string"),
        Some(&json!("640")),
        "the RM default, actual"
    );
    let (resource, losses) = outbound(&index, &composition)?;
    assert_eq!(resource.get("effectivePeriod"), Some(&period), "{resource}");
    assert_eq!(resource.get("effectiveDateTime"), None);
    assert!(
        !losses.iter().any(|loss| matches!(
            loss,
            Loss::Skipped {
                reason: SkipReason::EventFunctionUncarried { .. },
                ..
            }
        )),
        "an actual value loses no function: {losses:?}"
    );
    Ok(())
}

#[test]
fn an_interval_event_of_another_function_declares_it_lost() -> Result<(), Box<dyn Error>> {
    let index = single_event_template()?;
    let point = inbound(
        &index,
        &observation("effectiveDateTime", json!("2026-09-12T08:30:00+02:00")),
    )?;
    let node = index.node(&AqlPath::new(TIME))?;
    let composition = index.amend_holder(point, node, &[], |event| {
        event.insert(String::from("_type"), json!("INTERVAL_EVENT"));
        event.insert(
            String::from("width"),
            json!({"_type": "DV_DURATION", "value": "PT1H"}),
        );
        event.insert(
            String::from("math_function"),
            json!({
                "_type": "DV_CODED_TEXT",
                "value": "synthetic function",
                "defining_code": {
                    "_type": "CODE_PHRASE",
                    "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "openehr"},
                    "code_string": "146"
                }
            }),
        );
    })?;
    let (resource, losses) = outbound(&index, &composition)?;
    assert_eq!(
        resource.get("effectivePeriod"),
        Some(&json!({"start": "2026-09-12T07:30:00+02:00", "end": "2026-09-12T08:30:00+02:00"})),
        "{resource}"
    );
    assert!(
        losses.iter().any(|loss| matches!(
            loss,
            Loss::Skipped {
                reason: SkipReason::EventFunctionUncarried { code },
                ..
            } if code == "146"
        )),
        "{losses:?}"
    );
    Ok(())
}

#[test]
fn putget_holds_for_a_folded_point_and_interval_event() -> Result<(), Box<dyn Error>> {
    let index = single_event_template()?;
    let program = program(&index)?;
    let point = laws::putget(
        &program,
        &index,
        &observation("effectiveDateTime", json!("2026-09-12T08:30:00+02:00")),
        &laws::untouched,
    )?;
    insta::assert_json_snapshot!("folded_event_time_point_putget", point);
    let interval = laws::putget(
        &program,
        &index,
        &observation(
            "effectivePeriod",
            json!({"start": "2026-09-12T06:00:00+02:00", "end": "2026-09-12T08:30:00+02:00"}),
        ),
        &laws::untouched,
    )?;
    insta::assert_json_snapshot!("folded_event_time_interval_putget", interval);
    Ok(())
}

#[test]
fn getput_holds_for_a_folded_point_and_interval_event() -> Result<(), Box<dyn Error>> {
    let index = single_event_template()?;
    let program = program(&index)?;
    let point = inbound(
        &index,
        &observation("effectiveDateTime", json!("2026-09-12T08:30:00+02:00")),
    )?;
    insta::assert_json_snapshot!(
        "folded_event_time_point_getput",
        laws::getput(&program, &index, &point, &laws::untouched)?
    );
    let interval = inbound(
        &index,
        &observation(
            "effectivePeriod",
            json!({"start": "2026-09-12T06:00:00+02:00", "end": "2026-09-12T08:30:00+02:00"}),
        ),
    )?;
    insta::assert_json_snapshot!(
        "folded_event_time_interval_getput",
        laws::getput(&program, &index, &interval, &laws::untouched)?
    );
    // NOTE: the GetPut law compares FLAT forms, so the width the event holds is asserted
    // on the canonical document too.
    let (resource, _) = outbound(&index, &interval)?;
    let again = inbound(&index, &resource)?;
    assert_eq!(event_of(&index, &again)?, event_of(&index, &interval)?);
    Ok(())
}
