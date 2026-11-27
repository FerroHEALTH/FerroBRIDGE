// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The elements of a received discharge report the composition does not
//! carry, named in the composition's feeder system audit beside the original
//! content that keeps them.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::openehr::{DECLARATION, NOT_CARRIED, ReceivedComposition};
use openehr_mapping_core::index::WebTemplateIndex;
use serde_json::Value;

use super::{composition, diagnosis, mapping, sent, settings, template};

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
