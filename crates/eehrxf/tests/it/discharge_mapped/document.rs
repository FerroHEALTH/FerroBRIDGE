// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The discharge report document a mapping run is taken into, held to its profiles, and the runs it refuses.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use crate::support::emit;
use crate::support::hdr;
use crate::support::profile_set;
use crate::support::vendored_base;
use eehrxf::category::Category;
use eehrxf::crosswalk::discharge::HDR_COMPOSITION;
use eehrxf::document::DocumentError;
use eehrxf::document::discharge::HDR_BUNDLE;
use eehrxf::document::discharge::HDR_ENCOUNTER;
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::conform::check_resource;
use ferrobridge_term::client::Client;
use fhir_types::codec::Json;
use fhir_types::r4::bundle::Bundle;
use serde_json::Value;
use std::error::Error;

use super::{
    ADMITTED, DIAGNOSIS, NOW, composition, diagnosis, discharge_document, full_report, mapping,
    sent,
};
#[test]
fn the_mapped_report_is_a_discharge_document_its_profiles_constrain() -> Result<(), Box<dyn Error>>
{
    let text = sent(
        &mapping()?,
        &composition(&[diagnosis()]),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d102",
    )?;
    let document = ReceivedDocument::read(&text)?;
    assert_eq!(document.category()?, Category::Discharge);
    document.check(&hdr(HDR_BUNDLE)?, &hdr(HDR_COMPOSITION)?)?;
    let root: Value = serde_json::from_str(&text)?;
    let composition = &root["entry"][0]["resource"];
    let encounter = composition["encounter"]["reference"]
        .as_str()
        .ok_or("the composition names its encounter")?;
    let section = composition["section"]
        .as_array()
        .and_then(|sections| {
            sections
                .iter()
                .find(|section| section["code"]["coding"][0]["code"] == "11535-2")
        })
        .ok_or("the diagnostic summary section")?;
    let entry = section["entry"][0]["reference"]
        .as_str()
        .ok_or("the section's entry")?;
    let entries = root["entry"].as_array().ok_or("the entries")?;
    let resolve = |reference: &str| {
        entries.iter().find(|held| {
            held["fullUrl"]
                .as_str()
                .is_some_and(|url| url.ends_with(&format!("/{reference}")) || url == reference)
        })
    };
    let condition = resolve(entry).ok_or("the entry resolves")?;
    assert_eq!(
        condition["resource"]["code"]["coding"][0]["code"],
        DIAGNOSIS
    );
    let stay = resolve(encounter).ok_or("the encounter resolves")?;
    assert_eq!(stay["resource"]["meta"]["profile"][0], HDR_ENCOUNTER);
    assert_eq!(stay["resource"]["period"]["start"], ADMITTED);
    emit("eehrxf-discharge-mapped", &text)?;
    Ok(())
}

fn bundle_of(resources: &[Value]) -> Result<Bundle, Box<dyn Error>> {
    let entries: Vec<Value> = resources
        .iter()
        .map(|resource| serde_json::json!({ "resource": resource }))
        .collect();
    Ok(serde_json::from_value(serde_json::json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": entries
    }))?)
}

#[test]
fn a_run_without_a_composition_is_refused() -> Result<(), Box<dyn Error>> {
    let (mut document, _, _) = discharge_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000d108")?;
    let run = bundle_of(&[serde_json::json!({
        "resourceType": "Condition",
        "id": "synthetic-condition",
        "code": { "text": "Synthetic diagnosis" },
        "subject": { "reference": "Patient/synthetic-patient" }
    })])?;
    assert!(matches!(
        document.with_mapped_composition(&run),
        Err(DocumentError::Uncomposed { found: 0 })
    ));
    Ok(())
}

#[test]
fn a_section_whose_code_names_no_hdr_slot_is_refused() -> Result<(), Box<dyn Error>> {
    let (mut document, _, _) = discharge_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000d109")?;
    let run = bundle_of(&[serde_json::json!({
        "resourceType": "Composition",
        "status": "final",
        "type": { "coding": [{ "system": "http://loinc.org", "code": "34105-7" }] },
        "date": NOW,
        "author": [{ "display": "Synthetic author" }],
        "title": "Discharge report",
        "section": [{
            "title": "Medication summary",
            "code": { "coding": [{ "system": "http://loinc.org", "code": "10160-0" }] }
        }]
    })])?;
    match document.with_mapped_composition(&run) {
        Err(DocumentError::Unslotted { codes }) => {
            assert_eq!(codes, ["10160-0"]);
            Ok(())
        }
        other => Err(format!("the foreign section was placed: {other:?}").into()),
    }
}

#[test]
fn an_entry_the_run_does_not_hold_is_refused() -> Result<(), Box<dyn Error>> {
    let (mut document, _, _) = discharge_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000d110")?;
    let run = bundle_of(&[serde_json::json!({
        "resourceType": "Composition",
        "status": "final",
        "type": { "coding": [{ "system": "http://loinc.org", "code": "34105-7" }] },
        "date": NOW,
        "author": [{ "display": "Synthetic author" }],
        "title": "Discharge report",
        "section": [{
            "title": "Diagnostic summary",
            "code": { "coding": [{ "system": "http://loinc.org", "code": "11535-2" }] },
            "entry": [{ "reference": "Condition/absent" }]
        }]
    })])?;
    assert!(matches!(
        document.with_mapped_composition(&run),
        Err(DocumentError::Dangling {
            slice: "sectionDiagnosticSummary"
        })
    ));
    Ok(())
}

#[test]
fn every_entry_of_the_mapped_report_meets_the_profile_it_claims() -> Result<(), Box<dyn Error>> {
    let text = sent(
        &mapping()?,
        &full_report(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d114",
    )?;
    let document = ReceivedDocument::read(&text)?;
    let mut checked = Vec::new();
    for resource in document
        .bundle()
        .entry
        .iter()
        .filter_map(|entry| entry.resource.as_ref())
    {
        let object = Json::to_json(resource)?;
        let (
            Some(fhir_types::codec::Value::String(kind)),
            Some(fhir_types::codec::Value::Object(meta)),
        ) = (object.get("resourceType"), object.get("meta"))
        else {
            continue;
        };
        let Some(fhir_types::codec::Value::Array(profiles)) = meta.get("profile") else {
            continue;
        };
        for profile in profiles {
            let fhir_types::codec::Value::String(url) = profile else {
                continue;
            };
            let read = if url.starts_with("http://hl7.eu/fhir/hdr/") {
                hdr(url)?
            } else {
                vendored_base(url)?
            };
            let expected: &'static str = match kind.as_str() {
                "Composition" => "Composition",
                "Encounter" => "Encounter",
                "Condition" => "Condition",
                "Procedure" => "Procedure",
                "MedicationRequest" => "MedicationRequest",
                "Patient" => "Patient",
                other => return Err(format!("an unexpected {other} claims {url}").into()),
            };
            check_resource(&read, expected, &object)
                .map_err(|error| format!("{kind} against {url}: {error:?}"))?;
            checked.push(kind.clone());
        }
    }
    checked.sort();
    assert_eq!(
        checked,
        [
            "Composition",
            "Condition",
            "Encounter",
            "MedicationRequest",
            "Patient",
            "Procedure"
        ],
        "every claimed profile of every entry is checked"
    );
    Ok(())
}

#[test]
fn the_mapped_report_validates_against_bundle_eu_hdr() -> Result<(), Box<dyn Error>> {
    let Some(set) = profile_set()? else {
        return Ok(());
    };
    let text = sent(
        &mapping()?,
        &full_report(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d115",
    )?;
    let document = ReceivedDocument::read(&text)?;
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    runtime
        .block_on(document.validate(&set, HDR_BUNDLE, None::<&Client>))
        .map_err(|error| format!("{error:?}"))?;
    Ok(())
}
