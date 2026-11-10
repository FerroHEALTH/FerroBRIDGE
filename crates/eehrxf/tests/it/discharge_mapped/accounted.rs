// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The sections and entry elements of a received discharge report the HDR
//! project carries into no structured element, each declared in the run's
//! outcome at its FHIRPath.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::receive::ReceivedDocument;
use serde_json::Value;

use super::{composition, diagnosis, full_report, mapping, sent, settings};

/// The issues of the run over the received `text`, each as its FHIRPath and
/// its text.
fn issues(text: &str) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let received = mapping()?.to_openehr(&ReceivedDocument::read(text)?, &settings())?;
    let outcome = serde_json::to_value(received.outcome().ok_or("the run declares its losses")?)?;
    Ok(outcome["issue"]
        .as_array()
        .ok_or("the issues")?
        .iter()
        .map(|issue| {
            (
                issue["expression"][0]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                issue["details"]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect())
}

/// The element paths the issues of `issues` declare not carried.
fn not_carried(issues: &[(String, String)]) -> Vec<&str> {
    issues
        .iter()
        .filter(|(_, text)| text.contains("no structured element"))
        .map(|(path, _)| path.as_str())
        .collect()
}

/// The index of the section of the sent document coded `code`.
fn section(document: &Value, code: &str) -> Result<usize, Box<dyn Error>> {
    document["entry"][0]["resource"]["section"]
        .as_array()
        .ok_or("the sections")?
        .iter()
        .position(|section| section["code"]["coding"][0]["code"] == code)
        .ok_or_else(|| format!("the section {code}").into())
}

// NOTE: composition-eu-hdr cmp-hdr-2 has nearly every report carry the hospital course (LOINC
// 8648-8), which the template holds no section for, nor a medication order's status or intent.
#[test]
fn a_hospital_course_and_a_severity_are_declared_not_carried() -> Result<(), Box<dyn Error>> {
    let text = sent(
        &mapping()?,
        &full_report(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d120",
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
    condition["verificationStatus"] = serde_json::json!({"coding": [{
        "system": "http://terminology.hl7.org/CodeSystem/condition-ver-status",
        "code": "confirmed"
    }]});
    let course = section(&document, "8648-8")?;
    let issues = issues(&serde_json::to_string(&document)?)?;
    assert_eq!(
        not_carried(&issues),
        [
            format!("Composition.section[{course}]").as_str(),
            "Condition.verificationStatus",
            "Condition.severity",
            "MedicationRequest.status",
            "MedicationRequest.intent",
        ],
        "the hospital course once, each entry element the project does not read, and nothing \
         the project carries: {issues:?}"
    );
    for element in ["Condition.severity", "Condition.verificationStatus.coding"] {
        assert!(
            issues
                .iter()
                .any(|(path, text)| path == element && text.contains("did not run")),
            "the rule with no FHIR-to-openEHR direction stays declared apart: {issues:?}"
        );
    }
    for code in ["11535-2", "10185-7", "75311-1"] {
        let mapped = format!("Composition.section[{}]", section(&document, code)?);
        assert!(
            !not_carried(&issues).contains(&mapped.as_str()),
            "{code}: a section whose entries are carried is not declared"
        );
    }
    Ok(())
}

#[test]
fn a_section_whose_entries_are_carried_is_not_declared() -> Result<(), Box<dyn Error>> {
    let text = sent(
        &mapping()?,
        &composition(&[diagnosis(), diagnosis()]),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d121",
    )?;
    let document: Value = serde_json::from_str(&text)?;
    let course = section(&document, "8648-8")?;
    let issues = issues(&text)?;
    assert_eq!(
        not_carried(&issues),
        [format!("Composition.section[{course}]").as_str()],
        "{issues:?}"
    );
    Ok(())
}
