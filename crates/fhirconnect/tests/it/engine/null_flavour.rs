// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ELEMENT.null_flavour` against the FHIR `data-absent-reason` extension.
//!
//! FHIRconnect `types-of-mappings/data-type/NULL_FLAVOUR.adoc` §Data absent
//! maps the null flavour of an element to the extension's code, with the code
//! kept in `null_reason`. The mapping names the problem name element of the
//! synthetic diagnosis template and the extension on `Condition.code`.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::SkipReason;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;

use crate::engine::defaults;
use crate::support::compiled;
use crate::support::template;

/// The path of the problem name value in the synthetic template.
const PROBLEM: &str =
    "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0002]/value";

/// The extension that carries a data-absent reason.
const EXTENSION: &str = "http://hl7.org/fhir/StructureDefinition/data-absent-reason";

/// Returns a synthetic `Condition` whose code carries the data-absent reason
/// `code` and the text `text`.
fn condition(code: &str) -> Result<Value, Box<dyn Error>> {
    let mut parsed: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)?;
    let object = parsed.as_object_mut().ok_or("the Condition is an object")?;
    object.insert(
        String::from("code"),
        serde_json::json!({
            "extension": [{"url": EXTENSION, "valueCode": code}],
        }),
    );
    Ok(Value::from_serde_json(parsed))
}

/// Returns the problem name element the composition holds.
fn element_of(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let node = index.node(&AqlPath::new(PROBLEM))?;
    Ok(index
        .read_element(composition, node, &[])?
        .ok_or("the run wrote the element")?)
}

/// Runs `document` into openEHR, returning the composition.
fn inbound(document: &Value) -> Result<CanonicalComposition, Box<dyn Error>> {
    let program = compiled("ferrobridge_null_flavour")?;
    let index = template()?;
    Ok(to_openehr(
        &program,
        &SCHEMAS,
        &index,
        document,
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )?
    .into_value())
}

#[test]
fn a_data_absent_reason_travels_as_a_null_flavour_and_back() -> Result<(), Box<dyn Error>> {
    // NULL_FLAVOUR.adoc: `asked-declined` is "unknown", 253, with the code
    // kept in `null_reason`.
    let composition = inbound(&condition("asked-declined")?)?;
    let index = template()?;
    let element = element_of(&index, &composition)?;
    assert_eq!(
        element
            .pointer("/null_flavour/defining_code/code_string")
            .and_then(serde_json::Value::as_str),
        Some("253"),
        "the element holds the null flavour: {element}"
    );
    assert_eq!(
        element
            .pointer("/null_reason/value")
            .and_then(serde_json::Value::as_str),
        Some("asked-declined")
    );
    let program = compiled("ferrobridge_null_flavour")?;
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
    assert_eq!(
        resource.pointer("/code/extension/0"),
        Some(&serde_json::json!({"url": EXTENSION, "valueCode": "asked-declined"})),
        "the reason reads back: {resource}"
    );
    assert!(
        outbound.warnings().is_empty(),
        "nothing was lost: {:?}",
        outbound.warnings()
    );
    Ok(())
}

#[test]
fn a_code_outside_the_value_set_is_refused_going_in() -> Result<(), Box<dyn Error>> {
    let refused = inbound(&condition("not-a-reason")?)
        .err()
        .ok_or("the bridge is closed")?;
    let cause = Error::source(refused.as_ref()).ok_or("the refusal carries the cell's cause")?;
    assert!(cause.to_string().contains("not-a-reason"), "{cause}");
    Ok(())
}

#[test]
fn a_free_text_reason_is_declared_going_out() -> Result<(), Box<dyn Error>> {
    // NULL_FLAVOUR.adoc reads null_reason back only when it matches a code.
    let composition = inbound(&condition("asked-declined")?)?;
    let template_id = String::from(composition.template_id());
    let generation = composition.generation();
    let mut value = composition.into_value();
    let replaced = replace_reason(&mut value, "a synthetic free-text reason");
    assert!(replaced, "the composition holds the reason");
    let composition = CanonicalComposition::new(value, template_id, generation);
    let program = compiled("ferrobridge_null_flavour")?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &template()?,
        &composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let resource = outbound
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Condition"))?;
    assert_eq!(
        resource.pointer("/code/extension/0/valueCode"),
        Some(&serde_json::json!("unknown")),
        "253 reads back as unknown: {resource}"
    );
    let reasons: Vec<&SkipReason> = outbound
        .warnings()
        .iter()
        .filter_map(|warning| match *warning.loss() {
            Loss::Skipped { ref reason, .. } => Some(reason),
            _ => None,
        })
        .collect();
    assert_eq!(reasons, vec![&SkipReason::NullReasonUncarried]);
    Ok(())
}

/// Replaces the text of every `null_reason` in `value`, returning whether
/// one was found.
fn replace_reason(value: &mut serde_json::Value, text: &str) -> bool {
    match *value {
        serde_json::Value::Object(ref mut object) => {
            let mut found = false;
            if let Some(reason) = object.get_mut("null_reason") {
                *reason = serde_json::json!({"_type": "DV_TEXT", "value": text});
                found = true;
            }
            object
                .values_mut()
                .fold(found, |found, member| replace_reason(member, text) || found)
        }
        serde_json::Value::Array(ref mut items) => {
            let mut found = false;
            for item in items.iter_mut() {
                found |= replace_reason(item, text);
            }
            found
        }
        _ => false,
    }
}
