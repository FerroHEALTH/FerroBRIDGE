// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A scalar `Integer` or `Real` attribute below a template node against FHIR
//! `integer` or `decimal`.
//!
//! The data-type chapter leaves primitive pairs out as "straight forward"
//! (`types-of-mappings/data-type/data-mappings.adoc`). The template is the
//! synthetic diagnosis template with its problem name value turned into the
//! class under test, so a tail names its `magnitude`.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::engine::defaults;
use crate::support::compiled_against;

/// The archetype path of the problem name value the test retypes.
const PROBLEM: &str =
    "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0002]/value";

/// Returns the diagnosis template with the problem name value a `class`.
fn retyped_template(class: &str) -> Result<WebTemplateIndex, Box<dyn Error>> {
    let source = ferrobridge_testkit::fixtures::DIAGNOSE_OPT;
    let (head, rest) = source
        .split_once("<node_id>at0002</node_id>")
        .ok_or("the template carries the problem name")?;
    let text = "<rm_type_name>DV_TEXT</rm_type_name>";
    let (before, after) = rest
        .split_once(text)
        .ok_or("the problem name admits a text")?;
    let xml = format!(
        "{head}<node_id>at0002</node_id>{before}<rm_type_name>{class}</rm_type_name>{after}"
    );
    let opt = openehr_its::opt14::from_xml(&xml)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Returns a synthetic `Observation` whose value is `value` under `key`.
fn observation(key: &str, value: serde_json::Value) -> Result<Value, Box<dyn Error>> {
    let mut parsed: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_OBSERVATION)?;
    let object = parsed
        .as_object_mut()
        .ok_or("the Observation is an object")?;
    object.remove("valueQuantity");
    object.insert(String::from(key), value);
    Ok(Value::from_serde_json(parsed))
}

/// Runs `document` into openEHR and back out, returning the value written at
/// the problem name and the Observation read back.
fn round_trip(
    fixture: &str,
    class: &str,
    document: &Value,
) -> Result<(serde_json::Value, serde_json::Value), Box<dyn Error>> {
    let index = retyped_template(class)?;
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
    let node = index
        .nodes()
        .find(|node| node.aql_path().as_str() == PROBLEM)
        .ok_or("the template holds the problem name value")?;
    let written = index
        .read(&composition, node, &[])?
        .ok_or("the run wrote the value")?;
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
        .to_serde_json(&mut fhir_types::codec::Path::root("Observation"))?;
    Ok((written, resource))
}

#[test]
fn an_integer_travels_through_a_scalar_integer_leaf() -> Result<(), Box<dyn Error>> {
    // RM 1.1.0 data_types §DV_COUNT: `magnitude` is an Integer64.
    let (written, resource) = round_trip(
        "ferrobridge_scalar_integer",
        "DV_COUNT",
        &observation("valueInteger", serde_json::json!(7))?,
    )?;
    assert_eq!(written.get("magnitude"), Some(&serde_json::json!(7)));
    assert_eq!(resource.get("valueInteger"), Some(&serde_json::json!(7)));
    Ok(())
}

#[test]
fn a_decimal_travels_through_a_scalar_real_leaf() -> Result<(), Box<dyn Error>> {
    // RM 1.1.0 data_types §DV_QUANTITY: `magnitude` is a Real.
    let quantity = serde_json::json!({"value": 5.4, "unit": "mmol/L"});
    let (written, resource) = round_trip(
        "ferrobridge_scalar_real",
        "DV_QUANTITY",
        &observation("valueQuantity", quantity.clone())?,
    )?;
    assert_eq!(written.get("magnitude"), Some(&serde_json::json!(5.4)));
    assert_eq!(resource.get("valueQuantity"), Some(&quantity));
    Ok(())
}

#[test]
fn a_decimal_whose_places_a_real_cannot_keep_is_refused() -> Result<(), Box<dyn Error>> {
    // R4 datatypes §decimal: the precision of the decimal is significant, and
    // a reference-model Real keeps none.
    let index = retyped_template("DV_QUANTITY")?;
    let program = compiled_against("ferrobridge_scalar_real", &index)?;
    let text = ferrobridge_testkit::fixtures::R4_OBSERVATION.replacen(
        "\"value\": 5.4,",
        "\"value\": 5.40,",
        1,
    );
    assert!(
        text.contains("5.40"),
        "the fixture carries the quantity value"
    );
    let document: Value = serde_json::from_str(&text)?;
    let refused = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &document,
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )
    .err()
    .ok_or("5.40 is refused rather than rounded to 5.4")?;
    let cause = Error::source(&refused).ok_or("the refusal carries the cell's cause")?;
    assert!(
        cause.to_string().contains("5.40"),
        "the cause names the decimal: {cause}"
    );
    Ok(())
}
