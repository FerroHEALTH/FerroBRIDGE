// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `DV_INTERVAL<T>` node, which the Web Template names with its parameter.
//!
//! The template is the synthetic diagnosis template with its problem name
//! value turned into an interval, so the node's type name carries the bound
//! class (`DV_INTERVAL<DV_QUANTITY>`), as a published operational template
//! writes it.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::engine::defaults;
use crate::support::compiled_against;

/// The archetype path of the problem name the test turns into an interval.
const PROBLEM: &str =
    "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0002]/value";

/// Returns the diagnosis template with the problem name value a
/// `DV_INTERVAL` of `bound`.
fn interval_template(bound: &str) -> Result<WebTemplateIndex, Box<dyn Error>> {
    let source = ferrobridge_testkit::fixtures::DIAGNOSE_OPT;
    let (head, rest) = source
        .split_once("<node_id>at0002</node_id>")
        .ok_or("the template carries the problem name")?;
    let text = "<rm_type_name>DV_TEXT</rm_type_name>";
    let (before, after) = rest
        .split_once(text)
        .ok_or("the problem name admits a text")?;
    let xml = format!(
        "{head}<node_id>at0002</node_id>{before}<rm_type_name>DV_INTERVAL&lt;{bound}&gt;</rm_type_name>{after}"
    );
    let opt = openehr_its::opt14::from_xml(&xml)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Returns a synthetic `Condition` whose onset is `onset` under `key`.
fn condition(key: &str, onset: serde_json::Value) -> Result<Value, Box<dyn Error>> {
    let mut parsed: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)?;
    let object = parsed.as_object_mut().ok_or("the Condition is an object")?;
    object.remove("onsetDateTime");
    object.insert(String::from(key), onset);
    Ok(Value::from_serde_json(parsed))
}

/// Returns the interval the composition holds at the problem name.
fn interval_of(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let node = index
        .nodes()
        .find(|node| node.aql_path().as_str() == PROBLEM)
        .ok_or("the template holds the problem name value")?;
    Ok(index
        .read(composition, node, &[])?
        .ok_or("the run wrote the interval")?)
}

/// Runs `document` through the `fixture` mappings into openEHR and back out.
fn round_trip(
    fixture: &str,
    bound: &str,
    document: &Value,
) -> Result<(serde_json::Value, serde_json::Value), Box<dyn Error>> {
    let index = interval_template(bound)?;
    let program = compiled_against(fixture, &index)?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        document,
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )?;
    let composition = inbound.into_value();
    let written = interval_of(&index, &composition)?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let resource = outbound
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Condition"))?;
    Ok((written, resource))
}

#[test]
fn a_tail_below_a_generic_interval_node_compiles() -> Result<(), Box<dyn Error>> {
    // RM 1.1.0 data_types §DV_INTERVAL: `lower` and `upper` are of the
    // parameter `T`, so `/lower` below `DV_INTERVAL<DV_QUANTITY>` is a
    // DV_QUANTITY and runs the DV_QUANTITY.adoc §Quantity cell.
    let program = compiled_against(
        "ferrobridge_interval_tail",
        &interval_template("DV_QUANTITY")?,
    )?;
    let rendered = program.to_string();
    assert!(
        rendered.contains("DV_INTERVAL<DV_QUANTITY> tail lower\n    derived Quantity"),
        "the bound converts as a quantity: {rendered}"
    );
    Ok(())
}

#[test]
fn a_bound_below_a_generic_interval_node_travels_both_ways() -> Result<(), Box<dyn Error>> {
    let range = serde_json::json!({
        "low": {"value": 2, "unit": "a", "system": "http://unitsofmeasure.org", "code": "a"},
        "high": {"value": 5, "unit": "a", "system": "http://unitsofmeasure.org", "code": "a"},
    });
    let (written, resource) = round_trip(
        "ferrobridge_interval_tail",
        "DV_QUANTITY",
        &condition("onsetRange", range.clone())?,
    )?;
    assert_eq!(
        written
            .pointer("/lower/_type")
            .and_then(serde_json::Value::as_str),
        Some("DV_QUANTITY"),
        "the lower bound is a quantity: {written}"
    );
    assert_eq!(
        written
            .pointer("/upper/units")
            .and_then(serde_json::Value::as_str),
        Some("a"),
        "the upper bound keeps its units: {written}"
    );
    assert_eq!(resource.get("onsetRange"), Some(&range));
    Ok(())
}

#[test]
fn an_attribute_the_interval_parameter_lacks_is_refused_at_load() -> Result<(), Box<dyn Error>> {
    // RM 1.1.0 data_types §DV_QUANTITY declares no `numerator`.
    let refused = compiled_against(
        "ferrobridge_interval_tail_unknown",
        &interval_template("DV_QUANTITY")?,
    )
    .err()
    .ok_or("a numerator below a quantity bound is refused")?;
    assert!(
        refused.to_string().contains("numerator"),
        "the refusal names the attribute: {refused}"
    );
    Ok(())
}

#[test]
fn a_quantity_interval_picks_the_range_alternative() -> Result<(), Box<dyn Error>> {
    // DV_INTERVAL.adoc §Range: a DV_INTERVAL<DV_QUANTITY> pairs with Range,
    // so the untyped Condition.onset[x] is written as onsetRange.
    let range = serde_json::json!({
        "low": {"value": 2, "unit": "a", "system": "http://unitsofmeasure.org", "code": "a"},
        "high": {"value": 5, "unit": "a", "system": "http://unitsofmeasure.org", "code": "a"},
    });
    let (written, resource) = round_trip(
        "ferrobridge_onset_choice",
        "DV_QUANTITY",
        &condition("onsetRange", range.clone())?,
    )?;
    assert_eq!(
        written
            .pointer("/lower/_type")
            .and_then(serde_json::Value::as_str),
        Some("DV_QUANTITY"),
        "{written}"
    );
    assert_eq!(resource.get("onsetRange"), Some(&range), "{resource}");
    assert_eq!(resource.get("onsetPeriod"), None);
    Ok(())
}

#[test]
fn a_date_time_interval_picks_the_period_alternative() -> Result<(), Box<dyn Error>> {
    // data-mappings.adoc: "if a period is provided FHIRconnect maps this
    // period to an interval".
    let period = serde_json::json!({
        "start": "2026-09-01T08:00:00+02:00",
        "end": "2026-09-02T08:00:00+02:00",
    });
    let (written, resource) = round_trip(
        "ferrobridge_onset_choice",
        "DV_DATE_TIME",
        &condition("onsetPeriod", period.clone())?,
    )?;
    assert_eq!(
        written
            .pointer("/upper/value")
            .and_then(serde_json::Value::as_str),
        Some("2026-09-02T08:00:00+02:00"),
        "{written}"
    );
    assert_eq!(resource.get("onsetPeriod"), Some(&period), "{resource}");
    assert_eq!(resource.get("onsetRange"), None);
    Ok(())
}
