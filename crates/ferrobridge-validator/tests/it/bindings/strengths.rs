// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Each binding strength on each bindable type, against value sets the
//! package set enumerates (terminologies.html §4.1.5 and §4.1.6).

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::issue::Severity;
use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use serde_json::json;

use crate::bindings::COLORS;
use crate::bindings::VS;
use crate::bindings::binding;
use crate::bindings::binding_listings;
use crate::bindings::bound;
use crate::bindings::check;
use crate::bindings::value_set;
use crate::support::TestResult;
use crate::support::issues;
use crate::support::locations;

/// A Patient with `members` merged in.
fn patient(members: &serde_json::Value) -> serde_json::Value {
    let mut patient = json!({"resourceType": "Patient"});
    if let (Some(target), Some(source)) = (patient.as_object_mut(), members.as_object()) {
        target.extend(source.clone());
    }
    patient
}

/// A coding of [`COLORS`].
fn color(code: &str) -> serde_json::Value {
    json!({"system": COLORS, "code": code})
}

#[test]
fn a_required_code_outside_the_set_is_an_error() -> TestResult {
    let definition = bound(
        "Patient.gender",
        "code",
        "1",
        &binding("required", "listed"),
    );
    let report = check(&definition, &[], &patient(&json!({"gender": "red"})))?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    assert!(
        binding_listings(&report).is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    let report = check(&definition, &[], &patient(&json!({"gender": "purple"})))?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.gender"]
    );
    assert!(!report.is_valid());
    Ok(())
}

#[test]
fn a_required_coding_is_held_by_system_and_code() -> TestResult {
    let definition = bound(
        "Patient.meta.tag",
        "Coding",
        "*",
        &binding("required", "listed"),
    );
    let held = patient(&json!({"meta": {"tag": [color("blue")]}}));
    assert!(check(&definition, &[], &held)?.is_valid());
    let outside = patient(&json!({"meta": {"tag": [color("green")]}}));
    let report = check(&definition, &[], &outside)?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.meta.tag[0]"]
    );
    Ok(())
}

#[test]
fn a_required_coding_without_a_system_is_from_no_value_set() -> TestResult {
    let definition = bound(
        "Patient.meta.tag",
        "Coding",
        "*",
        &binding("required", "listed"),
    );
    let report = check(
        &definition,
        &[],
        &patient(&json!({"meta": {"tag": [{"code": "red"}]}})),
    )?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.meta.tag[0]"]
    );
    Ok(())
}

#[test]
fn a_required_codeable_concept_needs_one_coding_from_the_set() -> TestResult {
    let definition = bound(
        "Patient.maritalStatus",
        "CodeableConcept",
        "1",
        &binding("required", "listed"),
    );
    let one = patient(&json!({"maritalStatus": {"coding": [
        {"system": "http://example.org/fhir/CodeSystem/other", "code": "x"}, color("red")
    ]}}));
    assert!(check(&definition, &[], &one)?.is_valid());
    let none = patient(&json!({"maritalStatus": {"coding": [color("green")]}}));
    let report = check(&definition, &[], &none)?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.maritalStatus"]
    );
    Ok(())
}

#[test]
fn text_is_no_substitute_for_the_code_of_a_required_binding() -> TestResult {
    let definition = bound(
        "Patient.maritalStatus",
        "CodeableConcept",
        "1",
        &binding("required", "listed"),
    );
    let report = check(
        &definition,
        &[],
        &patient(&json!({"maritalStatus": {"text": "red"}})),
    )?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.maritalStatus"]
    );
    Ok(())
}

#[test]
fn a_binding_on_a_repeating_string_holds_each_value() -> TestResult {
    let definition = bound(
        "Patient.name.given",
        "string",
        "*",
        &binding("required", "listed"),
    );
    let resource = patient(&json!({"name": [{"given": ["red", "purple"]}]}));
    let report = check(&definition, &[], &resource)?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.name[0].given[1]"]
    );
    Ok(())
}

#[test]
fn a_uri_is_held_by_its_value_alone() -> TestResult {
    let uris = value_set(
        "uris",
        &json!({"include": [{"system": "urn:example:systems",
                             "concept": [{"code": "urn:example:mrn"}]}]}),
    );
    let definition = bound(
        "Patient.identifier.system",
        "uri",
        "1",
        &binding("required", "uris"),
    );
    let held = patient(&json!({"identifier": [{"system": "urn:example:mrn", "value": "1"}]}));
    assert!(check(&definition, std::slice::from_ref(&uris), &held)?.is_valid());
    let other = patient(&json!({"identifier": [{"system": "urn:example:other", "value": "1"}]}));
    let report = check(&definition, &[uris], &other)?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.identifier[0].system"]
    );
    Ok(())
}

#[test]
fn an_extensible_code_outside_the_set_is_a_warning() -> TestResult {
    let definition = bound(
        "Patient.gender",
        "code",
        "1",
        &binding("extensible", "listed"),
    );
    let report = check(&definition, &[], &patient(&json!({"gender": "purple"})))?;
    let found = issues(&report, MessageId::NotInExtensibleValueSet);
    assert_eq!(found.len(), 1, "{:#?}", report.issues());
    assert_eq!(found[0].severity, Severity::Warning);
    assert!(report.is_valid());
    Ok(())
}

#[test]
fn a_preferred_code_outside_the_set_is_information() -> TestResult {
    let definition = bound(
        "Patient.gender",
        "code",
        "1",
        &binding("preferred", "listed"),
    );
    let report = check(&definition, &[], &patient(&json!({"gender": "purple"})))?;
    let found = issues(&report, MessageId::NotInPreferredValueSet);
    assert_eq!(found.len(), 1, "{:#?}", report.issues());
    assert_eq!(found[0].severity, Severity::Information);
    Ok(())
}

#[test]
fn an_example_binding_is_not_checked_or_listed() -> TestResult {
    let definition = bound("Patient.gender", "code", "1", &binding("example", "listed"));
    let report = check(&definition, &[], &patient(&json!({"gender": "purple"})))?;
    assert!(report.issues().is_empty(), "{:#?}", report.issues());
    assert!(
        binding_listings(&report).is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    Ok(())
}

#[test]
fn the_maximum_value_set_is_held_as_a_required_binding() -> TestResult {
    let mut extensible = binding("extensible", "listed");
    extensible["extension"] = json!([{
        "url": "http://hl7.org/fhir/StructureDefinition/elementdefinition-maxValueSet",
        "valueCanonical": format!("{VS}every-color")
    }]);
    let definition = bound("Patient.gender", "code", "1", &extensible);
    let report = check(&definition, &[], &patient(&json!({"gender": "navy"})))?;
    assert_eq!(issues(&report, MessageId::NotInExtensibleValueSet).len(), 1);
    assert!(report.is_valid(), "navy is in the maximum value set");
    let report = check(&definition, &[], &patient(&json!({"gender": "purple"})))?;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.gender"]
    );
    assert!(!report.is_valid());
    Ok(())
}

#[test]
fn a_binding_without_a_value_set_is_named() -> TestResult {
    let definition = bound(
        "Patient.gender",
        "code",
        "1",
        &json!({"strength": "required", "description": "a description only"}),
    );
    let report = check(&definition, &[], &patient(&json!({"gender": "red"})))?;
    assert_eq!(
        binding_listings(&report),
        [(
            Constraint::Binding {
                strength: "required".to_owned(),
                value_set: None
            },
            Reason::BindingWithoutValueSet
        )]
    );
    Ok(())
}

#[test]
fn a_display_the_code_system_does_not_give_is_a_warning() -> TestResult {
    let definition = bound(
        "Patient.meta.tag",
        "Coding",
        "*",
        &binding("required", "every-color"),
    );
    let mut wrong = color("red");
    wrong["display"] = json!("Rouge");
    let mut designated = color("red");
    designated["display"] = json!("Crimson");
    let report = check(
        &definition,
        &[],
        &patient(&json!({"meta": {"tag": [designated, wrong]}})),
    )?;
    let found = issues(&report, MessageId::DisplayMismatch);
    assert_eq!(found.len(), 1, "{:#?}", report.issues());
    assert_eq!(found[0].expression, "Patient.meta.tag[1]");
    assert_eq!(found[0].severity, Severity::Warning);
    assert!(report.is_valid());
    Ok(())
}

#[test]
fn a_display_inside_a_codeable_concept_is_located_at_its_coding() -> TestResult {
    let definition = bound(
        "Patient.maritalStatus",
        "CodeableConcept",
        "1",
        &binding("extensible", "every-color"),
    );
    let mut wrong = color("blue");
    wrong["display"] = json!("Bleu");
    let resource = patient(&json!({"maritalStatus": {"coding": [color("red"), wrong]}}));
    let report = check(&definition, &[], &resource)?;
    assert_eq!(
        locations(&report, MessageId::DisplayMismatch),
        ["Patient.maritalStatus.coding[1]"]
    );
    Ok(())
}
