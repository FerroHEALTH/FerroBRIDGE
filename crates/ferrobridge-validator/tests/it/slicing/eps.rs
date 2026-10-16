// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A real slicing: HL7 Europe `composition-eu-eps` and `bundle-eu-eps`
//! 1.0.0-ballot over synthetic EPS-shaped documents, read with their
//! closure when `scripts/vendor/profile-packages.sh` has fetched the
//! packages; each test returns early otherwise.

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use serde_json::json;

use crate::eu::empty_reason;
use crate::eu::eu_set;
use crate::support::TestResult;
use crate::support::validate;

/// The Composition profile of the European patient summary.
const COMPOSITION: &str = "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps";

/// The Bundle profile of the European patient summary.
const BUNDLE: &str = "http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps";

/// A section coded with the LOINC code `code`, its entries referring to
/// `entries`.
fn section(code: &str, entries: &[&str]) -> serde_json::Value {
    let entry: Vec<_> = entries
        .iter()
        .map(|reference| json!({"reference": reference}))
        .collect();
    let mut section = json!({
        "title": code,
        "code": {"coding": [{"system": "http://loinc.org", "code": code}]},
        "text": {"status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">x</div>"}
    });
    // composition-eu-eps ips-comp-1: a required section carries entries or an emptyReason.
    if entry.is_empty() {
        section["emptyReason"] = empty_reason();
    } else {
        section["entry"] = json!(entry);
    }
    section
}

/// A synthetic EPS-shaped Composition with the sections coded `codes`.
fn composition(sections: &[serde_json::Value]) -> serde_json::Value {
    json!({
        "resourceType": "Composition",
        "identifier": {"system": "urn:ietf:rfc:3986", "value": "urn:uuid:00000000-0000-0000-0000-000000000001"},
        "status": "final",
        "type": {"coding": [{"system": "http://loinc.org", "code": "60591-5"}]},
        "subject": {"reference": "Patient/p1"},
        "date": "2026-01-01",
        "author": [{"reference": "Practitioner/a1"}],
        "title": "Patient summary",
        "section": sections
    })
}

/// The messages of the minimum-cardinality issues of `report`.
fn minimums(report: &ferrobridge_validator::report::Report) -> Vec<String> {
    report
        .with_message(MessageId::MinCardinality)
        .map(|issue| issue.message.clone())
        .collect()
}

#[test]
fn the_eps_sections_are_matched_by_their_code_pattern() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    let resource = composition(&[
        section("11450-4", &[]),
        section("48765-2", &[]),
        section("10160-0", &[]),
        section("47519-4", &[]),
    ]);
    let report = validate(&set, &resource, Some(COMPOSITION))?;
    let found = minimums(&report);
    assert!(
        found.iter().any(|message| message
            .starts_with("Composition.section:sectionMedicalDevices occurs 0 times")),
        "{found:#?}"
    );
    for matched in [
        "sectionProblems",
        "sectionAllergies",
        "sectionMedications",
        "sectionProceduresHx",
    ] {
        assert!(
            !found
                .iter()
                .any(|message| message.starts_with(&format!("Composition.section:{matched} "))),
            "{matched} holds its section: {found:#?}"
        );
    }
    Ok(())
}

#[test]
fn an_eps_section_entry_outside_the_composition_is_named_unevaluated() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    let resource = composition(&[section("11450-4", &["Condition/c1"])]);
    let report = validate(&set, &resource, Some(COMPOSITION))?;
    let entry = report.unevaluated().iter().find(|entry| {
        entry.element == "Composition.section:sectionProblems.entry"
            && matches!(&entry.constraint, Constraint::Discriminator { path, .. } if path == "$this.resolve()")
    });
    let entry = entry.expect("the resolve() discriminator is named");
    assert_eq!(entry.reason, Reason::ReferenceOutOfScope);
    assert_eq!(entry.location, "Composition.section[0].entry[0]");
    Ok(())
}

#[test]
fn an_eps_bundle_without_its_patient_entry_is_refused() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    let resource = json!({
        "resourceType": "Bundle",
        "identifier": {"system": "urn:ietf:rfc:3986", "value": "urn:uuid:00000000-0000-0000-0000-000000000002"},
        "type": "document",
        "timestamp": "2026-01-01T00:00:00Z",
        "entry": [{"fullUrl": "urn:uuid:00000000-0000-0000-0000-000000000003",
                   "resource": composition(&[section("11450-4", &[])])}]
    });
    let report = validate(&set, &resource, Some(BUNDLE))?;
    let found = minimums(&report);
    assert!(
        found.iter().any(|message| message == "Bundle.entry:patient occurs 0 times, below its minimum of 1"),
        "{found:#?}"
    );
    Ok(())
}

#[test]
fn an_eps_bundle_matches_its_entries_by_type_and_profile() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    let sections =
        ["11450-4", "48765-2", "10160-0", "47519-4", "46264-8"].map(|code| section(code, &[]));
    // documents.html §3.3.1: the subject and the author the Composition
    // references SHALL be in the document Bundle, so they name its entries.
    let mut composition = composition(&sections);
    composition["subject"] = json!({"reference": "urn:uuid:00000000-0000-0000-0000-000000000004"});
    composition["author"] = json!([{"reference": "urn:uuid:00000000-0000-0000-0000-000000000005"}]);
    let resource = json!({
        "resourceType": "Bundle",
        "identifier": {"system": "urn:ietf:rfc:3986", "value": "urn:uuid:00000000-0000-0000-0000-000000000002"},
        "type": "document",
        "timestamp": "2026-01-01T00:00:00Z",
        "entry": [
            {"fullUrl": "urn:uuid:00000000-0000-0000-0000-000000000003", "resource": composition},
            {"fullUrl": "urn:uuid:00000000-0000-0000-0000-000000000004", "resource": {
                "resourceType": "Patient", "identifier": [{"system": "urn:example:patients", "value": "1"}],
                "name": [{"family": "Doe", "given": ["Ada"]}], "gender": "female", "birthDate": "1970-01-01"}},
            {"fullUrl": "urn:uuid:00000000-0000-0000-0000-000000000005", "resource": {
                "resourceType": "Practitioner", "name": [{"family": "Smith", "given": ["Eve"]}]}}
        ]
    });
    let report = validate(&set, &resource, Some(BUNDLE))?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    Ok(())
}
