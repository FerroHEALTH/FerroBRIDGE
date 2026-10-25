// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_ORDINAL` against FHIR `CodeableConcept`
//! (`types-of-mappings/data-type/DV_ORDINAL.adoc` §`CodeableConcept`): the
//! coding carries the symbol, and the value comes from the options the
//! template's `C_DV_ORDINAL` lists (the openEHR OPT 1.4 XML schema).
//!
//! The template is the synthetic diagnosis template with its problem name
//! value turned into a `DV_ORDINAL` of two synthetic `local` options.

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

/// The fixture pair the test compiles.
const FIXTURE: &str = "ferrobridge_ordinal";

/// Returns one `C_DV_ORDINAL` list entry of `value` with the `local` symbol
/// `code`.
fn entry(value: i32, code: &str, rubric: &str) -> String {
    format!(
        "<list><symbol><value>{rubric}</value><defining_code><terminology_id>\
         <value>local</value></terminology_id><code_string>{code}</code_string>\
         </defining_code></symbol><value>{value}</value></list>"
    )
}

/// Returns the diagnosis template with the problem name value a `DV_ORDINAL`
/// of the options `Mild` (1) and `Severe` (3).
fn ordinal_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let source = ferrobridge_testkit::fixtures::DIAGNOSE_OPT;
    let (head, rest) = source
        .split_once("<node_id>at0002</node_id>")
        .ok_or("the template carries the problem name")?;
    let opening = "<children xsi:type=\"C_COMPLEX_OBJECT\">\n                    <rm_type_name>DV_TEXT</rm_type_name>";
    let (before, after) = rest
        .split_once(opening)
        .ok_or("the problem name admits a text")?;
    let (_, tail) = after
        .split_once("</children>")
        .ok_or("the text value closes")?;
    let ordinal = format!(
        "<children xsi:type=\"C_DV_ORDINAL\"><rm_type_name>DV_ORDINAL</rm_type_name>\
         <occurrences><lower_included>true</lower_included><upper_included>true</upper_included>\
         <lower_unbounded>false</lower_unbounded><upper_unbounded>false</upper_unbounded>\
         <lower>1</lower><upper>1</upper></occurrences><node_id></node_id>{}{}</children>",
        entry(1, "at9001", "Mild"),
        entry(3, "at9002", "Severe"),
    );
    let problem =
        "<term_definitions code=\"at0002\">\n          <items id=\"text\">Problem name</items>";
    let xml = format!("{head}<node_id>at0002</node_id>{before}{ordinal}{tail}");
    if !xml.contains(problem) {
        return Err("the template no longer spells its problem name term as expected".into());
    }
    let xml = xml.replacen(
        problem,
        &format!(
            "<term_definitions code=\"at9001\">\n          <items id=\"text\">Mild</items>\n          \
             <items id=\"description\">The first synthetic option</items>\n        </term_definitions>\n        \
             <term_definitions code=\"at9002\">\n          <items id=\"text\">Severe</items>\n          \
             <items id=\"description\">The second synthetic option</items>\n        </term_definitions>\n        {problem}"
        ),
        1,
    );
    let opt = openehr_its::opt14::from_xml(&xml)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Returns a synthetic `Observation` whose value is a `CodeableConcept` with
/// the `local` code `code`.
fn observation(code: &str, display: &str) -> Result<Value, Box<dyn Error>> {
    let mut parsed: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_OBSERVATION)?;
    let object = parsed
        .as_object_mut()
        .ok_or("the Observation is an object")?;
    object.remove("valueQuantity");
    object.insert(
        String::from("valueCodeableConcept"),
        serde_json::json!({
            "coding": [{"system": "local", "code": code, "display": display}],
            "text": display
        }),
    );
    Ok(Value::from_serde_json(parsed))
}

#[test]
fn a_codeable_concept_writes_the_ordinal_the_template_lists_and_reads_back()
-> Result<(), Box<dyn Error>> {
    let index = ordinal_template()?;
    let program = compiled_against(FIXTURE, &index)?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &observation("at9002", "Severe")?,
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
    assert_eq!(
        Some(&serde_json::json!(3)),
        written.get("value"),
        "{written}"
    );
    assert_eq!(
        Some(&serde_json::json!("at9002")),
        written.pointer("/symbol/defining_code/code_string"),
        "{written}"
    );
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
    assert_eq!(
        Some(&serde_json::json!("at9002")),
        resource.pointer("/valueCodeableConcept/coding/0/code"),
        "{resource}"
    );
    assert_eq!(
        Some(&serde_json::json!("Severe")),
        resource.pointer("/valueCodeableConcept/text"),
        "{resource}"
    );
    Ok(())
}

#[test]
fn a_code_the_template_does_not_list_is_refused() -> Result<(), Box<dyn Error>> {
    let index = ordinal_template()?;
    let program = compiled_against(FIXTURE, &index)?;
    let refused = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &observation("at9999", "Unlisted")?,
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    );
    let error = refused.err().ok_or("an unlisted symbol is refused")?;
    assert!(
        error.to_string().contains("DV_ORDINAL") || format!("{error:?}").contains("DV_ORDINAL"),
        "{error:?}"
    );
    Ok(())
}
