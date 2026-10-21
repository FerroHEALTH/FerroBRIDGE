// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FHIR time mapped onto an `EVENT` node itself.
//!
//! `types-of-mappings/data-type/EVENT.adoc` §`POINT_EVENT` and
//! `INTERVAL_EVENT`: a `time`, `date`, `dateTime` or `instant` makes a
//! `POINT_EVENT`, a `Period` an `INTERVAL_EVENT`. The RM fixes the rest:
//! `EVENT.time` is "the time point of the trailing edge of the event",
//! `interval_start_time = time - width`, and `math_function` defaults to
//! `640|actual|` (openEHR RM Release 1.1.0, data structures). The template is
//! the synthetic observation with events; the mapping is
//! `ferrobridge_event_time`, which maps `Observation.effective[x]` onto the
//! `EVENT` node as the published `laboratory_test_result.v1.yml` does.

use core::error::Error;

use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::SkipReason;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::error::EventRefusal;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use serde_json::json;

use crate::events::events_template;
use crate::events::readings;
use crate::laws;
use crate::support::compiled_against;

/// The synthetic observation.
const OBSERVATION: &str = "/content[openEHR-EHR-OBSERVATION.ferrobridge_reading.v1]";

/// The event node of the synthetic observation.
const EVENT: &str =
    "/content[openEHR-EHR-OBSERVATION.ferrobridge_reading.v1]/data[at0001]/events[at0002]";

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

/// Runs `resource` into openEHR and returns the composition, or the engine's
/// refusal as the inner error.
fn inbound(
    index: &WebTemplateIndex,
    resource: &serde_json::Value,
) -> Result<Result<CanonicalComposition, EngineError>, Box<dyn Error>> {
    let program = compiled_against("ferrobridge_event_time", index)?;
    Ok(to_openehr(
        &program,
        &SCHEMAS,
        index,
        &Value::from_serde_json(resource.clone()),
        &Seams::default(),
        &laws::defaults(),
        &CallContext::new(),
    )
    .map(fhirconnect::engine::outcome::Outcome::into_value))
}

/// Returns the first event the composition holds.
fn event_of(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let node = index.node(&AqlPath::new(EVENT))?;
    Ok(index
        .read(composition, node, &[RmPosition::first()])?
        .ok_or("the composition holds the event")?)
}

/// Runs `composition` out to FHIR and returns the resource with the losses.
fn outbound(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<(serde_json::Value, Vec<Loss>), Box<dyn Error>> {
    let program = compiled_against("ferrobridge_event_time", index)?;
    let out = to_fhir(
        &program,
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

/// Returns a composition of one point event at `time` whose history's origin
/// is the instant the law's runs default it to, since the mapping carries no
/// origin.
fn point_reading(
    index: &WebTemplateIndex,
    time: &str,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let built = readings(index, &[(time, 70)])?;
    let observation = index.node(&AqlPath::new(OBSERVATION))?;
    Ok(index.amend(built, observation, &[], |entry| {
        if let Some(origin) = entry
            .get_mut("data")
            .and_then(|data| data.get_mut("origin"))
        {
            *origin = json!({"_type": "DV_DATE_TIME", "value": laws::NOW});
        }
    })?)
}

/// Returns a composition of one event at `end`, made an `INTERVAL_EVENT` of
/// `width` whose function is `code`.
fn interval_reading(
    index: &WebTemplateIndex,
    end: &str,
    width: &str,
    code: &str,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let point = point_reading(index, end)?;
    let node = index.node(&AqlPath::new(EVENT))?;
    Ok(index.amend(point, node, &[RmPosition::first()], |event| {
        event.insert(String::from("_type"), json!("INTERVAL_EVENT"));
        event.insert(
            String::from("width"),
            json!({"_type": "DV_DURATION", "value": width}),
        );
        event.insert(
            String::from("math_function"),
            json!({
                "_type": "DV_CODED_TEXT",
                "value": "synthetic function",
                "defining_code": {
                    "_type": "CODE_PHRASE",
                    "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "openehr"},
                    "code_string": code
                }
            }),
        );
    })?)
}

#[test]
fn the_mapping_derives_an_event_time_with_both_alternatives() -> Result<(), Box<dyn Error>> {
    let program = compiled_against("ferrobridge_event_time", &events_template()?)?;
    let rendered = program.to_string();
    assert!(
        rendered.contains("event time read")
            && rendered.contains("as(DateTime)")
            && rendered.contains("as(Period)"),
        "{rendered}"
    );
    Ok(())
}

#[test]
fn a_date_time_goes_in_as_a_point_event_and_comes_back() -> Result<(), Box<dyn Error>> {
    let index = events_template()?;
    let composition = inbound(
        &index,
        &observation("effectiveDateTime", json!("2026-09-12T08:30:00+02:00")),
    )??;
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
fn an_instant_and_a_date_go_in_as_the_time_of_a_point_event() -> Result<(), Box<dyn Error>> {
    // R4 datatypes §dateTime: both are forms of the dateTime lexical space.
    let index = events_template()?;
    for (key, text) in [
        ("effectiveInstant", "2026-09-12T08:30:00.250+02:00"),
        ("effectiveDateTime", "2026-09-12"),
    ] {
        let composition = inbound(&index, &observation(key, json!(text)))??;
        let event = event_of(&index, &composition)?;
        assert_eq!(event.get("_type"), Some(&json!("POINT_EVENT")), "{key}");
        assert_eq!(event.pointer("/time/value"), Some(&json!(text)), "{key}");
        let (resource, _) = outbound(&index, &composition)?;
        assert_eq!(
            resource.get("effectiveDateTime"),
            Some(&json!(text)),
            "{key}"
        );
    }
    Ok(())
}

#[test]
fn a_period_goes_in_as_an_interval_event_and_comes_back() -> Result<(), Box<dyn Error>> {
    let index = events_template()?;
    let period = json!({"start": "2026-09-12T06:00:00+02:00", "end": "2026-09-12T08:30:00+02:00"});
    let composition = inbound(&index, &observation("effectivePeriod", period.clone()))??;
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
    let index = events_template()?;
    let composition = interval_reading(&index, "2026-09-12T08:30:00+02:00", "PT1H", "146")?;
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
fn a_period_with_a_missing_bound_is_refused() -> Result<(), Box<dyn Error>> {
    let index = events_template()?;
    let refused = inbound(
        &index,
        &observation(
            "effectivePeriod",
            json!({"end": "2026-09-12T08:30:00+02:00"}),
        ),
    )?;
    assert!(
        matches!(
            refused,
            Err(EngineError::EventTime {
                reason: EventRefusal::OpenPeriod,
                ..
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_period_that_ends_before_it_starts_is_refused() -> Result<(), Box<dyn Error>> {
    let index = events_template()?;
    let refused = inbound(
        &index,
        &observation(
            "effectivePeriod",
            json!({"start": "2026-09-12T09:00:00+02:00", "end": "2026-09-12T08:30:00+02:00"}),
        ),
    )?;
    assert!(
        matches!(
            refused,
            Err(EngineError::EventTime {
                reason: EventRefusal::NoWidth,
                ..
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_timing_is_paired_with_no_event_class() -> Result<(), Box<dyn Error>> {
    let index = events_template()?;
    let refused = inbound(
        &index,
        &observation(
            "effectiveTiming",
            json!({"event": ["2026-09-12T08:30:00+02:00"]}),
        ),
    )?;
    assert!(
        matches!(
            refused,
            Err(EngineError::EventTime {
                reason: EventRefusal::Type,
                ..
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn putget_holds_for_a_point_and_an_interval_event() -> Result<(), Box<dyn Error>> {
    let index = events_template()?;
    let program = compiled_against("ferrobridge_event_time", &index)?;
    let point = laws::putget(
        &program,
        &index,
        &observation("effectiveDateTime", json!("2026-09-12T08:30:00+02:00")),
        &laws::untouched,
    )?;
    insta::assert_json_snapshot!("event_time_point_putget", point);
    let interval = laws::putget(
        &program,
        &index,
        &observation(
            "effectivePeriod",
            json!({"start": "2026-09-12T06:00:00+02:00", "end": "2026-09-12T08:30:00+02:00"}),
        ),
        &laws::untouched,
    )?;
    insta::assert_json_snapshot!("event_time_interval_putget", interval);
    Ok(())
}

#[test]
fn getput_holds_for_a_point_and_an_interval_event() -> Result<(), Box<dyn Error>> {
    let index = events_template()?;
    let program = compiled_against("ferrobridge_event_time", &index)?;
    let point = laws::getput(
        &program,
        &index,
        &point_reading(&index, "2026-09-12T08:30:00+02:00")?,
        &laws::untouched,
    )?;
    insta::assert_json_snapshot!("event_time_point_getput", point);
    let interval = laws::getput(
        &program,
        &index,
        &interval_reading(&index, "2026-09-12T08:30:00+02:00", "PT2H30M", "640")?,
        &laws::untouched,
    )?;
    insta::assert_json_snapshot!("event_time_interval_getput", interval);
    Ok(())
}
