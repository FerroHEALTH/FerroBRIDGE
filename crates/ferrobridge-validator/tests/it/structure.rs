// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The structural checks: unknown members, the JSON shapes of
//! <https://hl7.org/fhir/R4/json.html> §2.6.2, choice types and
//! `contentReference`.

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::issue::Severity;
use ferrobridge_validator::set::PackageSet;
use serde_json::json;

use crate::support::TestResult;
use crate::support::against_patient;
use crate::support::core;
use crate::support::locations;
use crate::support::patient;
use crate::support::validate;

#[test]
fn the_conforming_patient_raises_no_error() -> TestResult {
    let report = against_patient(&patient())?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn an_unknown_member_is_an_error_at_its_location() -> TestResult {
    let mut resource = patient();
    resource["name"][0]["nickname"] = json!("Ace");
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::UnknownElement),
        ["Patient.name[0].nickname"]
    );
    Ok(())
}

#[test]
fn an_unknown_member_inside_a_primitive_sibling_is_an_error() -> TestResult {
    let mut resource = patient();
    resource["_gender"] = json!({"comment": "x"});
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::UnknownElement),
        ["Patient.gender.comment"]
    );
    Ok(())
}

#[test]
fn a_sibling_for_a_complex_element_is_unknown() -> TestResult {
    let mut resource = patient();
    resource["_maritalStatus"] = json!({"id": "m"});
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::UnknownElement),
        ["Patient._maritalStatus"]
    );
    Ok(())
}

#[test]
fn a_repeating_element_written_as_a_single_value_is_refused() -> TestResult {
    let mut resource = patient();
    resource["name"] = json!({"family": "Doe"});
    let report = against_patient(&resource)?;
    assert_eq!(locations(&report, MessageId::WrongShape), ["Patient.name"]);
    Ok(())
}

#[test]
fn a_single_element_written_as_an_array_is_refused() -> TestResult {
    let mut resource = patient();
    resource["gender"] = json!(["female"]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::WrongShape),
        ["Patient.gender"]
    );
    Ok(())
}

#[test]
fn an_empty_object_is_refused() -> TestResult {
    let mut resource = patient();
    resource["maritalStatus"] = json!({});
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::EmptyObject),
        ["Patient.maritalStatus"]
    );
    Ok(())
}

#[test]
fn an_empty_array_is_refused() -> TestResult {
    let mut resource = patient();
    resource["name"][0]["given"] = json!([]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::EmptyArray),
        ["Patient.name[0].given"]
    );
    Ok(())
}

#[test]
fn a_null_no_aligned_array_needs_is_refused() -> TestResult {
    let mut resource = patient();
    resource["name"][0]["given"] = json!(["Ada", null]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::NullValue),
        ["Patient.name[0].given[1]"]
    );
    Ok(())
}

#[test]
fn a_null_paired_with_a_sibling_entry_is_accepted() -> TestResult {
    let mut resource = patient();
    resource["name"][0]["given"] = json!(["Ada", null]);
    resource["name"][0]["_given"] = json!([null, {"id": "g2"}]);
    let report = against_patient(&resource)?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn misaligned_primitive_arrays_are_refused() -> TestResult {
    let mut resource = patient();
    resource["name"][0]["given"] = json!(["Ada", "Bea"]);
    resource["name"][0]["_given"] = json!([{"id": "g1"}]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::MisalignedArrays),
        ["Patient.name[0].given"]
    );
    Ok(())
}

#[test]
fn a_choice_type_the_profile_does_not_allow_is_refused() -> TestResult {
    let mut resource = patient();
    resource
        .as_object_mut()
        .expect("an object")
        .remove("deceasedBoolean");
    resource["deceasedDateTime"] = json!("2020-01-01");
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::ChoiceTypeNotAllowed),
        ["Patient.deceased.ofType(dateTime)"]
    );
    Ok(())
}

#[test]
fn two_forms_of_one_choice_are_refused() -> TestResult {
    let mut resource = patient();
    resource["deceasedDateTime"] = json!("2020-01-01");
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::DuplicateChoice),
        ["Patient.deceased"]
    );
    Ok(())
}

#[test]
fn an_unknown_resource_type_is_fatal() -> TestResult {
    let set = PackageSet::load([core()])?;
    let report = validate(&set, &json!({"resourceType": "Patience"}), None)?;
    let issue = report
        .with_message(MessageId::UnknownResourceType)
        .next()
        .expect("an issue");
    assert_eq!(issue.severity, Severity::Fatal);
    Ok(())
}

#[test]
fn a_profile_for_another_type_is_reported() -> TestResult {
    let report = against_patient(&json!({"resourceType": "Observation", "status": "final",
        "code": {"text": "t"}}))?;
    assert_eq!(
        locations(&report, MessageId::ProfileTypeMismatch),
        ["Observation"]
    );
    Ok(())
}

#[test]
fn a_content_reference_walks_the_target_definition() -> TestResult {
    // Questionnaire.item.item carries contentReference #Questionnaire.item
    // (elementdefinition.html#ElementDefinition.contentReference).
    let set = PackageSet::load([core()])?;
    let questionnaire = json!({
        "resourceType": "Questionnaire",
        "status": "draft",
        "item": [{"linkId": "1", "type": "group", "item": [{"type": "string", "colour": "red"}]}]
    });
    let report = validate(&set, &questionnaire, None)?;
    assert_eq!(
        locations(&report, MessageId::UnknownElement),
        ["Questionnaire.item[0].item[0].colour"]
    );
    assert_eq!(
        locations(&report, MessageId::MinCardinality),
        ["Questionnaire.item[0].item[0]"]
    );
    Ok(())
}

#[test]
fn a_contained_resource_is_walked_against_its_own_type() -> TestResult {
    let mut resource = patient();
    resource["contained"] = json!([{"resourceType": "Organization", "id": "o1", "colour": "red"}]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::UnknownElement),
        ["Patient.contained[0].colour"]
    );
    Ok(())
}
