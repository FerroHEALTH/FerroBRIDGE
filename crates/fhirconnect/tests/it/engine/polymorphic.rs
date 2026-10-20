// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An untyped FHIR choice element against an `ELEMENT` whose value admits
//! several data types.
//!
//! The data-type chapter pairs each class with FHIR types
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/`),
//! so the alternative a document carries selects the value node going in, and
//! the value node a composition holds selects the alternative coming out. The
//! template is the synthetic diagnosis template with its problem name turned
//! into an `ELEMENT` whose value admits `DV_TEXT` and `DV_DATE_TIME`.

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

/// The archetype path of the problem name the test makes polymorphic.
const PROBLEM: &str =
    "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0002]";

/// Returns the diagnosis template with the problem name an `ELEMENT` whose
/// value admits `DV_TEXT` and `DV_DATE_TIME`: the operational template gains
/// a second alternative under the `ELEMENT`'s `value`.
fn polymorphic_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let source = ferrobridge_testkit::fixtures::DIAGNOSE_OPT;
    let (head, rest) = source
        .split_once("<node_id>at0002</node_id>")
        .ok_or("the template carries the problem name")?;
    let text = "<rm_type_name>DV_TEXT</rm_type_name>";
    let (before, after) = rest
        .split_once(text)
        .ok_or("the problem name admits a text")?;
    let (constraint, tail) = after
        .split_once("</children>")
        .ok_or("the text constraint closes")?;
    let date_time = "<children xsi:type=\"C_COMPLEX_OBJECT\"><rm_type_name>DV_DATE_TIME</rm_type_name>\
        <occurrences><lower_included>true</lower_included><upper_included>true</upper_included>\
        <lower_unbounded>false</lower_unbounded><upper_unbounded>false</upper_unbounded>\
        <lower>1</lower><upper>1</upper></occurrences><node_id></node_id></children>";
    let xml = format!(
        "{head}<node_id>at0002</node_id>{before}{text}{constraint}</children>{date_time}{tail}"
    );
    let opt = openehr_its::opt14::from_xml(&xml)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Returns a synthetic `Condition` carrying `value` under its onset key.
fn condition(key: &str, value: &str) -> Result<Value, Box<dyn Error>> {
    let mut parsed: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)?;
    let object = parsed.as_object_mut().ok_or("the Condition is an object")?;
    object.remove("onsetDateTime");
    object.insert(
        String::from(key),
        serde_json::Value::String(String::from(value)),
    );
    Ok(Value::from_serde_json(parsed))
}

/// Returns the problem name value of `class` the composition holds.
fn problem_value(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    class: &str,
) -> Result<Option<serde_json::Value>, Box<dyn Error>> {
    let node = index
        .nodes()
        .find(|node| {
            node.rm_type() == class && node.aql_path().as_str() == format!("{PROBLEM}/value")
        })
        .ok_or("the template holds the value alternative")?;
    Ok(index.read(composition, node, &[])?)
}

/// Runs `document` into openEHR and back out.
fn round_trip(
    document: &Value,
) -> Result<(WebTemplateIndex, CanonicalComposition, serde_json::Value), Box<dyn Error>> {
    let index = polymorphic_template()?;
    let program = compiled_against("ferrobridge_onset_choice", &index)?;
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
    Ok((index, composition, resource))
}

#[test]
fn the_choice_derives_a_pair_per_value_alternative() -> Result<(), Box<dyn Error>> {
    let program = compiled_against("ferrobridge_onset_choice", &polymorphic_template()?)?;
    let rendered = program.to_string();
    assert!(
        rendered.contains("string into DV_TEXT") && rendered.contains("dateTime into DV_DATE_TIME"),
        "each alternative pairs with its class: {rendered}"
    );
    Ok(())
}

#[test]
fn a_date_time_goes_into_the_date_time_alternative_and_back() -> Result<(), Box<dyn Error>> {
    // DV_DATE_TIME.adoc pairs dateTime with DV_DATE_TIME.
    let (index, composition, resource) =
        round_trip(&condition("onsetDateTime", "2026-09-01T08:00:00+02:00")?)?;
    let written = problem_value(&index, &composition, "DV_DATE_TIME")?
        .ok_or("the date and time alternative holds the value")?;
    assert_eq!(
        written.get("value").and_then(serde_json::Value::as_str),
        Some("2026-09-01T08:00:00+02:00")
    );
    assert_eq!(
        written.get("_type").and_then(serde_json::Value::as_str),
        Some("DV_DATE_TIME")
    );
    assert_eq!(
        resource.get("onsetDateTime"),
        Some(&serde_json::json!("2026-09-01T08:00:00+02:00")),
        "the alternative the composition holds is the one written: {resource}"
    );
    assert_eq!(resource.get("onsetString"), None);
    Ok(())
}

#[test]
fn a_string_goes_into_the_text_alternative_and_back() -> Result<(), Box<dyn Error>> {
    // DV_TEXT.adoc §String pairs string with DV_TEXT.
    let (index, composition, resource) =
        round_trip(&condition("onsetString", "a synthetic onset")?)?;
    let written = problem_value(&index, &composition, "DV_TEXT")?
        .ok_or("the text alternative holds the value")?;
    assert_eq!(
        written.get("value").and_then(serde_json::Value::as_str),
        Some("a synthetic onset")
    );
    assert_eq!(
        written.get("_type").and_then(serde_json::Value::as_str),
        Some("DV_TEXT")
    );
    assert_eq!(
        resource.get("onsetString"),
        Some(&serde_json::json!("a synthetic onset"))
    );
    Ok(())
}

#[test]
fn an_alternative_no_value_node_pairs_with_refuses() -> Result<(), Box<dyn Error>> {
    let index = polymorphic_template()?;
    let program = compiled_against("ferrobridge_onset_choice", &index)?;
    let mut parsed: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)?;
    let object = parsed.as_object_mut().ok_or("the Condition is an object")?;
    object.remove("onsetDateTime");
    object.insert(
        String::from("onsetAge"),
        serde_json::json!({"value": 42, "unit": "a", "system": "http://unitsofmeasure.org", "code": "a"}),
    );
    let refused = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(parsed),
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    );
    assert!(refused.is_err(), "no value node pairs with an Age");
    Ok(())
}
