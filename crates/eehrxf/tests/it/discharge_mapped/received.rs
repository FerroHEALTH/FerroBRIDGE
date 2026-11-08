// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A received discharge report mapped into openEHR, and the report mapped back out of it.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use crate::support::emit;
use crate::support::hdr;
use eehrxf::crosswalk::discharge::HDR_COMPOSITION;
use eehrxf::document::discharge::HDR_BUNDLE;
use eehrxf::receive::ReceivedDocument;
use ferrobridge_testkit::fixtures::FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE_ID;
use fhir_types::r4::bundle::Bundle;
use serde_json::Value;
use std::error::Error;

use super::{
    ADMITTED, DIAGNOSIS, DISCHARGED, composition, diagnosis, full_report, mapping, only,
    section_entries, sent, settings,
};
#[test]
fn a_received_discharge_report_maps_into_one_composition_that_keeps_it()
-> Result<(), Box<dyn Error>> {
    let mapping = mapping()?;
    let text = sent(
        &mapping,
        &composition(&[diagnosis()]),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d103",
    )?;
    let received = mapping.to_openehr(&ReceivedDocument::read(&text)?, &settings())?;
    assert_eq!(
        received.template_id(),
        FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE_ID
    );
    let outcome = serde_json::to_value(received.outcome().ok_or("the run declares its losses")?)?;
    let declared: Vec<&str> = outcome["issue"]
        .as_array()
        .ok_or("the issues")?
        .iter()
        .filter_map(|issue| issue["details"]["text"].as_str())
        .collect();
    assert_eq!(
        declared,
        [
            "the mapping metaURL did not run: its unidirectional marker names the other direction",
            "the mapping status did not run: its unidirectional marker names the other direction",
            "the mapping date did not run: its unidirectional marker names the other direction",
            "the mapping author did not run: its unidirectional marker names the other direction",
            "the mapping type did not run: its unidirectional marker names the other direction",
            "the mapping title did not run: its unidirectional marker names the other direction",
            "the mapping encounter.encounterProfile did not run: its unidirectional marker names the other direction",
            "the mapping encounter.finished did not run: its unidirectional marker names the other direction",
            "the mapping encounter.inpatient did not run: its unidirectional marker names the other direction",
            "the mapping diagnosticSummary.title did not run: its unidirectional marker names the other direction",
            "the mapping diagnosticSummary.code did not run: its unidirectional marker names the other direction",
            "the mapping metaURL did not run: its unidirectional marker names the other direction",
            "the mapping severity did not run: its unidirectional marker names the other direction",
            "the mapping certainty did not run: its unidirectional marker names the other direction",
            "the engine filled /composer",
            "the engine filled /context/setting",
            "the engine filled /language",
            "the engine filled /territory",
        ],
        "every element the composition does not carry is declared, and nothing else"
    );
    let canonical: Value = serde_json::from_str(received.composition())?;
    assert_eq!(
        canonical
            .pointer("/feeder_audit/original_content/value")
            .and_then(Value::as_str),
        Some(text.as_str()),
        "the document is kept byte for byte"
    );
    assert_eq!(
        canonical.pointer("/context/start_time/value"),
        Some(&Value::from(ADMITTED))
    );
    assert_eq!(
        canonical.pointer("/context/end_time/value"),
        Some(&Value::from(DISCHARGED))
    );
    let written = canonical.to_string();
    for value in [DIAGNOSIS, "2026-10-11T22:00:00Z", "2026-10-12T10:15:00Z"] {
        assert!(written.contains(value), "{value} is mapped: {written}");
    }
    Ok(())
}

#[test]
fn the_profile_elements_of_the_discharge_report_survive_the_round_trip()
-> Result<(), Box<dyn Error>> {
    let mapping = mapping()?;
    let first = sent(
        &mapping,
        &composition(&[diagnosis()]),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d104",
    )?;
    let received = mapping.to_openehr(&ReceivedDocument::read(&first)?, &settings())?;
    let mut stored: Value = serde_json::from_str(received.composition())?;
    let object = stored
        .as_object_mut()
        .ok_or("the composition is an object")?;
    object.insert(
        String::from("uid"),
        serde_json::json!({
            "_type": "OBJECT_VERSION_ID",
            "value": "5b0c3f7e-0000-4000-8000-00000000d105::ferrobridge.example::1"
        }),
    );
    let second = sent(
        &mapping,
        &stored.to_string(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d106",
    )?;
    let document = ReceivedDocument::read(&second)?;
    document.check(&hdr(HDR_BUNDLE)?, &hdr(HDR_COMPOSITION)?)?;
    let (before, after) = (
        only(document_bundle(&first)?.as_ref(), "Condition")?,
        only(document.bundle(), "Condition")?,
    );
    for pointer in [
        "/meta/profile",
        "/code",
        "/onsetDateTime",
        "/extension",
        "/bodySite",
        "/note",
    ] {
        assert_eq!(before.pointer(pointer), after.pointer(pointer), "{pointer}");
    }
    let (before, after) = (
        only(document_bundle(&first)?.as_ref(), "Encounter")?,
        only(document.bundle(), "Encounter")?,
    );
    for pointer in ["/meta/profile", "/status", "/class", "/period"] {
        assert_eq!(before.pointer(pointer), after.pointer(pointer), "{pointer}");
    }
    Ok(())
}

/// The decoded document `Bundle` of the text `text`.
fn document_bundle(text: &str) -> Result<Box<Bundle>, Box<dyn Error>> {
    Ok(Box::new(ReceivedDocument::read(text)?.bundle().clone()))
}

#[test]
fn every_section_of_the_report_survives_the_round_trip() -> Result<(), Box<dyn Error>> {
    let mapping = mapping()?;
    let first = sent(
        &mapping,
        &full_report(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d111",
    )?;
    let received = ReceivedDocument::read(&first)?;
    received.check(&hdr(HDR_BUNDLE)?, &hdr(HDR_COMPOSITION)?)?;
    let stored = mapping.to_openehr(&received, &settings())?;
    let mut canonical: Value = serde_json::from_str(stored.composition())?;
    canonical
        .as_object_mut()
        .ok_or("the composition is an object")?
        .insert(
            String::from("uid"),
            serde_json::json!({
                "_type": "OBJECT_VERSION_ID",
                "value": "5b0c3f7e-0000-4000-8000-00000000d112::ferrobridge.example::1"
            }),
        );
    let second = sent(
        &mapping,
        &canonical.to_string(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d113",
    )?;
    let document = ReceivedDocument::read(&second)?;
    document.check(&hdr(HDR_BUNDLE)?, &hdr(HDR_COMPOSITION)?)?;
    for (kind, pointers) in [
        (
            "Procedure",
            &[
                "/meta/profile",
                "/status",
                "/code",
                "/performedDateTime",
                "/bodySite",
                "/note",
            ][..],
        ),
        (
            "MedicationRequest",
            &[
                "/meta/profile",
                "/status",
                "/intent",
                "/medicationCodeableConcept",
                "/dosageInstruction",
                "/note",
            ][..],
        ),
        (
            "Condition",
            &["/meta/profile", "/code", "/onsetDateTime"][..],
        ),
    ] {
        let (before, after) = (
            only(received.bundle(), kind)?,
            only(document.bundle(), kind)?,
        );
        for pointer in pointers {
            assert_eq!(
                before.pointer(pointer),
                after.pointer(pointer),
                "{kind}{pointer}"
            );
        }
    }
    let root: Value = serde_json::from_str(&second)?;
    let composition = &root["entry"][0]["resource"];
    for code in ["11535-2", "10185-7", "75311-1"] {
        assert_eq!(section_entries(composition, code).len(), 1, "{code}");
    }
    emit("eehrxf-discharge-full", &second)?;
    Ok(())
}
