// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The STU3 element table and the cardinality, types, type profiles and
//! primitive forms an STU3 profile sets.

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Report;
use serde_json::json;

use crate::stu3::errors;
use crate::stu3::stu3_profile;
use crate::stu3::stu3_set;
use crate::stu3::validate_stu3;
use crate::support::BASE;
use crate::support::TestResult;
use crate::support::locations;

/// An STU3 Patient profile: one or two names; `deceased[x]` restricted to
/// `boolean`; each name held to the `HumanName` profile `stu3-name`, named
/// by the single STU3 `type.profile`.
fn patient_profile() -> serde_json::Value {
    stu3_profile(
        "stu3-patient",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.name", "path": "Patient.name", "min": 1, "max": "2",
             "type": [{"code": "HumanName", "profile": format!("{BASE}stu3-name")}]},
            {"id": "Patient.deceased[x]", "path": "Patient.deceased[x]", "min": 0, "max": "1",
             "type": [{"code": "boolean"}]}
        ]),
    )
}

/// A `HumanName` profile whose `family` is required.
fn name_profile() -> serde_json::Value {
    stu3_profile(
        "stu3-name",
        "HumanName",
        &json!([
            {"id": "HumanName", "path": "HumanName", "min": 0, "max": "*"},
            {"id": "HumanName.family", "path": "HumanName.family", "min": 1, "max": "1",
             "type": [{"code": "string"}]}
        ]),
    )
}

/// Validates `resource` against the STU3 Patient profile.
fn against_patient(resource: &serde_json::Value) -> TestResult<Report> {
    let (_synthetic, set) = stu3_set(&[patient_profile(), name_profile()])?;
    validate_stu3(&set, resource, Some(&format!("{BASE}stu3-patient")))
}

#[test]
fn an_stu3_patient_conforming_to_its_profile_raises_no_error() -> TestResult {
    let report = against_patient(&json!({
        "resourceType": "Patient",
        "name": [{"family": "Doe"}],
        "deceasedBoolean": false,
        "animal": {"species": {"text": "dog"}}
    }))?;
    assert!(errors(&report).is_empty(), "{:#?}", errors(&report));
    Ok(())
}

#[test]
fn an_element_r4_added_is_unknown_to_an_stu3_observation() -> TestResult {
    // STU3 Observation carries `related`, which R4 replaced by `hasMember` and `derivedFrom`.
    let (_synthetic, set) = stu3_set(&[])?;
    let report = validate_stu3(
        &set,
        &json!({
            "resourceType": "Observation",
            "status": "final",
            "code": {"text": "weight"},
            "related": [{"target": {"display": "an earlier one"}}],
            "hasMember": [{"display": "a member"}]
        }),
        None,
    )?;
    assert_eq!(
        locations(&report, MessageId::UnknownElement),
        ["Observation.hasMember"]
    );
    Ok(())
}

#[test]
fn a_resource_type_r4_added_is_no_stu3_resource() -> TestResult {
    let (_synthetic, set) = stu3_set(&[])?;
    let report = validate_stu3(&set, &json!({"resourceType": "ServiceRequest"}), None)?;
    let found: Vec<String> = report
        .with_message(MessageId::UnknownResourceType)
        .map(|issue| issue.message.clone())
        .collect();
    assert_eq!(found, ["resourceType names no STU3 resource"]);
    Ok(())
}

#[test]
fn an_stu3_profile_holds_the_minimum_cardinality() -> TestResult {
    let report = against_patient(&json!({"resourceType": "Patient"}))?;
    assert_eq!(locations(&report, MessageId::MinCardinality), ["Patient"]);
    Ok(())
}

#[test]
fn an_stu3_profile_holds_the_maximum_cardinality() -> TestResult {
    let report = against_patient(&json!({
        "resourceType": "Patient",
        "name": [{"family": "A"}, {"family": "B"}, {"family": "C"}]
    }))?;
    assert_eq!(
        locations(&report, MessageId::MaxCardinality),
        ["Patient.name"]
    );
    Ok(())
}

#[test]
fn an_stu3_profile_limits_the_types_of_a_choice() -> TestResult {
    let report = against_patient(&json!({
        "resourceType": "Patient",
        "name": [{"family": "Doe"}],
        "deceasedDateTime": "2020-01-01"
    }))?;
    assert_eq!(
        locations(&report, MessageId::ChoiceTypeNotAllowed),
        ["Patient.deceased.ofType(dateTime)"]
    );
    Ok(())
}

#[test]
fn the_single_stu3_type_profile_holds_the_content() -> TestResult {
    let report = against_patient(&json!({
        "resourceType": "Patient",
        "name": [{"given": ["Ada"]}]
    }))?;
    assert_eq!(
        locations(&report, MessageId::MinCardinality),
        ["Patient.name[0]"]
    );
    Ok(())
}

#[test]
fn an_stu3_primitive_is_held_to_the_form_its_package_states() -> TestResult {
    // STU3 states the form of date.value in structuredefinition-regex (STU3 datatypes.html).
    let (_synthetic, set) = stu3_set(&[])?;
    let report = validate_stu3(
        &set,
        &json!({"resourceType": "Patient", "birthDate": "2020-13-01"}),
        None,
    )?;
    assert_eq!(
        locations(&report, MessageId::LexicalForm),
        ["Patient.birthDate"]
    );
    Ok(())
}

/// Validates an Observation containing `patient` as `#p`, its subject,
/// against a profile holding the subject to the single STU3
/// `targetProfile` `stu3-patient`.
fn with_subject(patient: serde_json::Value) -> TestResult<Report> {
    let observation = stu3_profile(
        "stu3-observation",
        "Observation",
        &json!([
            {"id": "Observation", "path": "Observation", "min": 0, "max": "*"},
            {"id": "Observation.subject", "path": "Observation.subject", "min": 1, "max": "1",
             "type": [{"code": "Reference", "targetProfile": format!("{BASE}stu3-patient")}]}
        ]),
    );
    let (_synthetic, set) = stu3_set(&[observation, patient_profile(), name_profile()])?;
    let mut contained = patient;
    contained["id"] = json!("p");
    let resource = json!({
        "resourceType": "Observation",
        "contained": [contained],
        "status": "final",
        "code": {"text": "weight"},
        "subject": {"reference": "#p"}
    });
    validate_stu3(&set, &resource, Some(&format!("{BASE}stu3-observation")))
}

#[test]
fn a_target_meeting_the_single_stu3_target_profile_passes() -> TestResult {
    let report = with_subject(json!({"resourceType": "Patient", "name": [{"family": "Doe"}]}))?;
    assert!(errors(&report).is_empty(), "{:#?}", errors(&report));
    Ok(())
}

#[test]
fn a_target_breaking_the_single_stu3_target_profile_is_refused() -> TestResult {
    let report = with_subject(json!({"resourceType": "Patient", "active": true}))?;
    assert_eq!(
        locations(&report, MessageId::TargetProfileMismatch),
        ["Observation.subject"]
    );
    Ok(())
}
