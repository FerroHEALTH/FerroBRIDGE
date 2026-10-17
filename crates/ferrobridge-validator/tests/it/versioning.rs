// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ElementDefinition.type.versioning`: "If type.versioning is present in
//! the definition, the 'reference' element SHALL be present and have a value
//! and the reference SHALL be populated as the versioning constraint
//! dictates" (<https://hl7.org/fhir/R4/elementdefinition.html>, §2.30.0.4.4).

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use serde_json::json;

use crate::support::BASE;
use crate::support::TestResult;
use crate::support::locations;
use crate::support::profile;
use crate::support::set_with;
use crate::support::validate;

/// Validates a Patient whose general practitioner is `reference` against a
/// profile setting `rule` on `Patient.generalPractitioner`.
fn practitioner(rule: &str, reference: &serde_json::Value) -> TestResult<Report> {
    let definition = profile(
        "versioned-gp",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.generalPractitioner", "path": "Patient.generalPractitioner",
             "min": 0, "max": "*",
             "type": [{"code": "Reference", "versioning": rule}]}
        ]),
    );
    let (_synthetic, set) = set_with(&[definition])?;
    let resource = json!({"resourceType": "Patient", "generalPractitioner": [reference]});
    validate(&set, &resource, Some(&format!("{BASE}versioned-gp")))
}

/// Validates a Questionnaire derived from `canonical` against a profile
/// setting `rule` on `Questionnaire.derivedFrom`.
fn derived(rule: &str, canonical: &str) -> TestResult<Report> {
    let definition = profile(
        "versioned-derivation",
        "Questionnaire",
        &json!([
            {"id": "Questionnaire", "path": "Questionnaire", "min": 0, "max": "*"},
            {"id": "Questionnaire.derivedFrom", "path": "Questionnaire.derivedFrom",
             "min": 0, "max": "*",
             "type": [{"code": "canonical", "versioning": rule}]}
        ]),
    );
    let (_synthetic, set) = set_with(&[definition])?;
    let resource =
        json!({"resourceType": "Questionnaire", "status": "draft", "derivedFrom": [canonical]});
    validate(
        &set,
        &resource,
        Some(&format!("{BASE}versioned-derivation")),
    )
}

/// The locations of the versioning refusals of `report`.
fn refused(report: &Report) -> Vec<String> {
    locations(report, MessageId::VersioningMismatch)
}

#[test]
fn a_specific_rule_refuses_a_reference_without_a_version() -> TestResult {
    let report = practitioner("specific", &json!({"reference": "Practitioner/p1"}))?;
    assert_eq!(refused(&report), ["Patient.generalPractitioner[0]"]);
    Ok(())
}

#[test]
fn a_specific_rule_admits_a_reference_with_a_version() -> TestResult {
    let report = practitioner(
        "specific",
        &json!({"reference": "Practitioner/p1/_history/3"}),
    )?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn an_independent_rule_refuses_a_reference_with_a_version() -> TestResult {
    let report = practitioner(
        "independent",
        &json!({"reference": "http://example.org/fhir/Practitioner/p1/_history/3"}),
    )?;
    assert_eq!(refused(&report), ["Patient.generalPractitioner[0]"]);
    Ok(())
}

#[test]
fn an_independent_rule_admits_a_reference_without_a_version() -> TestResult {
    let report = practitioner("independent", &json!({"reference": "Practitioner/p1"}))?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn a_versioning_rule_refuses_a_reference_without_a_literal_reference() -> TestResult {
    let report = practitioner("either", &json!({"display": "Dr. Smith"}))?;
    assert_eq!(refused(&report), ["Patient.generalPractitioner[0]"]);
    Ok(())
}

#[test]
fn a_specific_rule_assumes_a_reference_outside_a_rest_server_is_specific() -> TestResult {
    // references.html, Reference.reference: "If the reference is not to a FHIR RESTful
    // server, then it should be assumed to be version specific."
    let report = practitioner(
        "specific",
        &json!({"reference": "urn:uuid:00000000-0000-0000-0000-000000000001"}),
    )?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn an_independent_rule_lists_a_reference_outside_a_rest_server() -> TestResult {
    let report = practitioner(
        "independent",
        &json!({"reference": "urn:uuid:00000000-0000-0000-0000-000000000001"}),
    )?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    let entry = report
        .unevaluated()
        .iter()
        .find(|entry| matches!(&entry.constraint, Constraint::Versioning { rule } if rule == "independent"))
        .expect("the versioning is listed");
    assert_eq!(entry.reason, Reason::NotRestfulReference);
    assert_eq!(entry.location, "Patient.generalPractitioner[0]");
    Ok(())
}

#[test]
fn a_specific_rule_refuses_a_canonical_without_a_version() -> TestResult {
    let report = derived("specific", "http://example.org/fhir/Questionnaire/base")?;
    assert_eq!(refused(&report), ["Questionnaire.derivedFrom[0]"]);
    let report = derived("specific", "http://example.org/fhir/Questionnaire/base|2.0")?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn an_independent_rule_refuses_a_canonical_with_a_version() -> TestResult {
    let report = derived(
        "independent",
        "http://example.org/fhir/Questionnaire/base|2.0",
    )?;
    assert_eq!(refused(&report), ["Questionnaire.derivedFrom[0]"]);
    let report = derived("independent", "http://example.org/fhir/Questionnaire/base")?;
    assert!(refused(&report).is_empty(), "{:#?}", report.issues());
    Ok(())
}
