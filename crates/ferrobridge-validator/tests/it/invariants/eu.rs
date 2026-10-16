// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The invariants of the HL7 Europe EPS profiles 1.0.0-ballot,
//! `eps-bundle-subject-ref` on `bundle-eu-eps` and `ips-pat-1` on
//! `patient-eu-eps`, read when `scripts/vendor/profile-packages.sh` has
//! fetched the packages; each test returns early otherwise and fails under
//! CI ([`crate::support::profile_packages`]).

use serde_json::json;

use super::failed_at;
use crate::eu::empty_reason;
use crate::eu::eu_set;
use crate::support::TestResult;
use crate::support::validate;

/// The Bundle profile of the European patient summary.
const BUNDLE: &str = "http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps";

/// The Patient profile of the European patient summary.
const PATIENT: &str = "http://hl7.eu/fhir/eps/StructureDefinition/patient-eu-eps";

/// A synthetic EPS document whose Observation entry has a subject when
/// `subject`.
fn document(subject: bool) -> serde_json::Value {
    let patient = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000070";
    let author = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000071";
    let observation = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000072";
    let sections: Vec<serde_json::Value> = ["11450-4", "48765-2", "10160-0", "47519-4", "46264-8"]
        .iter()
        .map(|code| {
            json!({
                "title": code,
                "code": {"coding": [{"system": "http://loinc.org", "code": code}]},
                "text": {"status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">x</div>"},
                "emptyReason": empty_reason()
            })
        })
        .collect();
    let mut result = json!({
        "resourceType": "Observation", "status": "final", "code": {"text": "synthetic"}
    });
    if subject {
        result["subject"] = json!({"reference": patient});
    }
    json!({
        "resourceType": "Bundle",
        "identifier": {"system": "urn:ietf:rfc:3986", "value": "urn:uuid:4a1e8f9e-0000-4000-8000-000000000073"},
        "type": "document",
        "timestamp": "2026-01-01T00:00:00Z",
        "entry": [
            {"fullUrl": "urn:uuid:4a1e8f9e-0000-4000-8000-000000000074", "resource": {
                "resourceType": "Composition", "status": "final",
                "type": {"coding": [{"system": "http://loinc.org", "code": "60591-5"}]},
                "subject": {"reference": patient}, "date": "2026-01-01",
                "author": [{"reference": author}], "title": "Patient summary", "section": sections}},
            {"fullUrl": patient, "resource": {
                "resourceType": "Patient", "identifier": [{"system": "urn:example:patients", "value": "1"}],
                "name": [{"family": "Doe", "given": ["Ada"]}], "gender": "female", "birthDate": "1970-01-01"}},
            {"fullUrl": author, "resource": {
                "resourceType": "Practitioner", "name": [{"family": "Smith", "given": ["Eve"]}]}},
            {"fullUrl": observation, "resource": result}
        ]
    })
}

#[test]
fn eps_bundle_subject_ref_reports_an_entry_without_a_subject() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    let report = validate(&set, &document(true), Some(BUNDLE))?;
    assert!(
        failed_at(&report, "eps-bundle-subject-ref").is_empty(),
        "{:#?}",
        report.issues()
    );
    let report = validate(&set, &document(false), Some(BUNDLE))?;
    assert_eq!(failed_at(&report, "eps-bundle-subject-ref"), ["Bundle"]);
    Ok(())
}

#[test]
fn ips_pat_1_reports_a_name_without_a_family_given_or_text() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    let patient = |name: serde_json::Value| {
        json!({
            "resourceType": "Patient", "identifier": [{"system": "urn:example:patients", "value": "1"}],
            "name": [name], "gender": "female", "birthDate": "1970-01-01"
        })
    };
    let report = validate(&set, &patient(json!({"family": "Doe"})), Some(PATIENT))?;
    assert!(
        failed_at(&report, "ips-pat-1").is_empty(),
        "{:#?}",
        report.issues()
    );
    let report = validate(&set, &patient(json!({"use": "official"})), Some(PATIENT))?;
    assert_eq!(failed_at(&report, "ips-pat-1"), ["Patient.name[0]"]);
    Ok(())
}
