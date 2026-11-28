// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The elements of a received discharge report the composition does not
//! carry, named in the composition's feeder system audit beside the original
//! content that keeps them, and in the composition's comment, where the
//! record's reader sees them.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::mapping::{Mapping, MappingError};
use eehrxf::mappings::HDR;
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::openehr::{COMMENT_LEAD, DECLARATION, NOT_CARRIED, ReceivedComposition};
use ferrobridge_testkit::fixtures::FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE;
use openehr_mapping_core::index::WebTemplateIndex;
use serde_json::Value;

use super::{composition, diagnosis, mapping, sent, settings, template, template_of};

/// The composition the received report `text` maps into.
fn received(text: &str) -> Result<ReceivedComposition, Box<dyn Error>> {
    Ok(mapping()?.to_openehr(&ReceivedDocument::read(text)?, &settings())?)
}

/// A received report of one diagnosis whose `Condition` carries a
/// `severity`, an element the HDR project accounts for and does not read.
fn noted() -> Result<String, Box<dyn Error>> {
    let text = sent(
        &mapping()?,
        &composition(&[diagnosis()]),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d140",
    )?;
    let mut document: Value = serde_json::from_str(&text)?;
    let condition = document["entry"]
        .as_array_mut()
        .ok_or("the entries")?
        .iter_mut()
        .map(|entry| &mut entry["resource"])
        .find(|resource| resource["resourceType"] == "Condition")
        .ok_or("the Condition")?;
    condition["severity"] = serde_json::json!({"coding": [{
        "system": "http://snomed.info/sct",
        "code": "6736007",
        "display": "Moderate"
    }]});
    Ok(serde_json::to_string(&document)?)
}

// NOTE: RM Release 1.1.0, Common IM, FEEDER_AUDIT_DETAILS.other_details carries "any custom
// meta-data"; the declaration names each element by its FHIRPath and never by its value.
#[test]
fn an_element_the_composition_does_not_carry_is_named_in_its_feeder_audit()
-> Result<(), Box<dyn Error>> {
    let composition = received(&noted()?)?;
    assert!(
        composition
            .not_carried()
            .iter()
            .any(|path| path == "Condition.severity"),
        "{:?}",
        composition.not_carried()
    );
    let canonical: Value = serde_json::from_str(composition.composition())?;
    let audit = &canonical["feeder_audit"];
    assert_eq!(
        audit["original_content"]["formalism"], "application/fhir+json",
        "the original content keeps the received document"
    );
    let declared = &audit["feeder_system_audit"]["other_details"];
    assert_eq!(declared["name"]["value"], DECLARATION, "{declared}");
    let items = declared["items"]
        .as_array()
        .ok_or("the declared elements")?;
    let named: Vec<&str> = items
        .iter()
        .filter_map(|item| item["name"]["value"].as_str())
        .collect();
    assert_eq!(named, composition.not_carried(), "{declared}");
    for item in items {
        assert_eq!(item["value"]["value"], NOT_CARRIED, "{item}");
    }
    assert!(
        !declared.to_string().contains("6736007"),
        "an element is named by its path, never by its value: {declared}"
    );
    Ok(())
}

#[test]
fn the_composition_naming_its_losses_validates_against_its_template() -> Result<(), Box<dyn Error>>
{
    let composition = received(&noted()?)?;
    let index = WebTemplateIndex::build(&template()?)?;
    index.accept(serde_json::from_str(composition.composition())?)?;
    Ok(())
}

// NOTE: RM Release 1.1.0, EHR IM, EVENT_CONTEXT.other_context holds the template's Comment, the
// composition-level element its reader sees; it names each element by its FHIRPath alone.
#[test]
fn an_element_the_composition_does_not_carry_is_named_in_its_comment() -> Result<(), Box<dyn Error>>
{
    let composition = received(&noted()?)?;
    let canonical: Value = serde_json::from_str(composition.composition())?;
    let tree = &canonical["context"]["other_context"];
    assert_eq!(tree["archetype_node_id"], "at0001", "{tree}");
    let comments: Vec<&Value> = tree["items"]
        .as_array()
        .ok_or("the context's items")?
        .iter()
        .filter(|item| item["archetype_node_id"] == "at0.1")
        .collect();
    let [comment] = comments.as_slice() else {
        return Err(format!("one comment, not {}: {tree}", comments.len()).into());
    };
    assert_eq!(comment["name"]["value"], "Comment", "{comment}");
    assert_eq!(
        comment["value"]["value"],
        format!("{COMMENT_LEAD}{}", composition.not_carried().join("; ")),
        "{comment}"
    );
    assert!(
        comment["value"]["value"]
            .as_str()
            .is_some_and(|text| text.contains("Condition.severity") && !text.contains("6736007")),
        "{comment}"
    );
    Ok(())
}

#[test]
fn a_report_that_loses_nothing_has_no_comment() -> Result<(), Box<dyn Error>> {
    let document = ReceivedDocument::read(&noted()?)?;
    let composition = mapping()?
        .accounting(&[])
        .to_openehr(&document, &settings())?;
    assert!(
        composition.not_carried().is_empty(),
        "{:?}",
        composition.not_carried()
    );
    let canonical: Value = serde_json::from_str(composition.composition())?;
    let comments = canonical["context"]["other_context"]["items"]
        .as_array()
        .map_or(0, |items| {
            items
                .iter()
                .filter(|item| item["archetype_node_id"] == "at0.1")
                .count()
        });
    assert_eq!(comments, 0, "{canonical}");
    assert!(
        canonical["feeder_audit"]["feeder_system_audit"]["other_details"].is_null(),
        "{canonical}"
    );
    Ok(())
}

#[test]
fn a_template_without_the_comment_is_refused() -> Result<(), Box<dyn Error>> {
    let element = FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE
        .find("        /context/other_context[at0001]/items matches {")
        .ok_or("the comment's constraint")?;
    let content = FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE
        .find("        content matches {")
        .ok_or("the content constraint")?;
    let uncommented = format!(
        "{}{}",
        FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE
            .get(..element)
            .ok_or("the head")?,
        FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE
            .get(content..)
            .ok_or("the tail")?
    );
    let refused = Mapping::compile_project(&template_of(&uncommented)?, &HDR);
    assert!(
        matches!(refused, Err(MappingError::NoNote { ref path, .. })
            if path == "/context/other_context[at0001]/items[at0.1]/value"),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_comment_a_sent_report_leaves_out_is_named_in_the_hospital_course() {
    assert_eq!(
        HDR.slice_of("/context/other_context[at0001]/items[at0.1]/value"),
        Some("sectionHospitalCourse")
    );
    assert_eq!(
        HDR.slice_of("/context/other_context[at0001]/items[at0.10]/value"),
        None
    );
}
