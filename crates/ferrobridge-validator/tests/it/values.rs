// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Primitive values (<https://hl7.org/fhir/R4/datatypes.html> §2.24.0.1),
//! `maxLength`, `fixed[x]` and `pattern[x]`
//! (<https://hl7.org/fhir/R4/elementdefinition-definitions.html>).

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::set::PackageSet;
use serde_json::json;

use crate::support::TestResult;
use crate::support::against_patient;
use crate::support::core;
use crate::support::locations;
use crate::support::patient;
use crate::support::validate;

/// An Observation carrying `value` as its `valueX` member.
fn observation(member: &str, value: serde_json::Value) -> serde_json::Value {
    let mut observation =
        json!({"resourceType": "Observation", "status": "final", "code": {"text": "t"}});
    observation[member] = value;
    observation
}

#[test]
fn a_value_outside_the_lexical_form_of_its_type_is_refused() -> TestResult {
    let mut resource = patient();
    resource["birthDate"] = json!("yesterday");
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::LexicalForm),
        ["Patient.birthDate"]
    );
    Ok(())
}

/// R4 resource-definitions.html#Resource.id types the logical id as `id`, so
/// a space or a 65th character breaks it (datatypes.html#id).
#[test]
fn a_resource_id_outside_the_id_form_is_refused() -> TestResult {
    for id in ["a b", &"a".repeat(65)] {
        let mut resource = patient();
        resource["id"] = json!(id);
        let report = against_patient(&resource)?;
        assert_eq!(locations(&report, MessageId::LexicalForm), ["Patient.id"]);
    }
    let mut resource = patient();
    resource["id"] = json!(format!("{}-.", "A1".repeat(31)));
    let report = against_patient(&resource)?;
    assert!(locations(&report, MessageId::LexicalForm).is_empty());
    Ok(())
}

#[test]
fn a_lexical_form_of_a_choice_type_is_read_from_its_definition() -> TestResult {
    let set = PackageSet::load([core()])?;
    let report = validate(&set, &observation("valueTime", json!("25:00:00")), None)?;
    assert_eq!(
        locations(&report, MessageId::LexicalForm),
        ["Observation.value.ofType(time)"]
    );
    Ok(())
}

#[test]
fn an_integer_above_the_32_bit_range_is_refused() -> TestResult {
    let set = PackageSet::load([core()])?;
    let report = validate(
        &set,
        &observation("valueInteger", json!(2_147_483_648_i64)),
        None,
    )?;
    assert_eq!(
        locations(&report, MessageId::IntegerRange),
        ["Observation.value.ofType(integer)"]
    );
    Ok(())
}

#[test]
fn an_integer_with_a_fraction_is_refused() -> TestResult {
    let set = PackageSet::load([core()])?;
    let value: serde_json::Value = serde_json::from_str("1.5")?;
    let report = validate(&set, &observation("valueInteger", value), None)?;
    assert_eq!(
        locations(&report, MessageId::LexicalForm),
        ["Observation.value.ofType(integer)"]
    );
    Ok(())
}

#[test]
fn a_number_written_as_a_string_is_refused() -> TestResult {
    let set = PackageSet::load([core()])?;
    let report = validate(&set, &observation("valueInteger", json!("12")), None)?;
    assert_eq!(
        locations(&report, MessageId::WrongJsonType),
        ["Observation.value.ofType(integer)"]
    );
    Ok(())
}

#[test]
fn an_empty_string_is_refused() -> TestResult {
    let mut resource = patient();
    resource["name"][0]["given"] = json!([""]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::EmptyString),
        ["Patient.name[0].given[0]"]
    );
    Ok(())
}

#[test]
fn a_string_over_the_one_megabyte_limit_is_refused() -> TestResult {
    // datatypes.html §2.24.0.1: strings SHALL NOT exceed 1024*1024 characters,
    // which R4 core states as the maxLength of string.value.
    let mut resource = patient();
    resource["name"][0]["text"] = json!("a".repeat(1024 * 1024 + 1));
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::TooLong),
        ["Patient.name[0].text"]
    );
    Ok(())
}

#[test]
fn a_string_over_the_profile_max_length_is_refused() -> TestResult {
    let mut resource = patient();
    resource["name"][0]["family"] = json!("Lovelace");
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::MaxLength),
        ["Patient.name[0].family"]
    );
    Ok(())
}

#[test]
fn a_value_other_than_the_fixed_value_is_refused() -> TestResult {
    let mut resource = patient();
    resource["gender"] = json!("male");
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::FixedMismatch),
        ["Patient.gender"]
    );
    Ok(())
}

#[test]
fn a_fixed_primitive_with_an_extension_beside_it_matches() -> TestResult {
    let mut resource = patient();
    resource["_gender"] = json!({"id": "g"});
    let report = against_patient(&resource)?;
    assert!(
        locations(&report, MessageId::FixedMismatch).is_empty(),
        "{:#?}",
        report.issues()
    );
    Ok(())
}

#[test]
fn a_value_without_the_pattern_is_refused() -> TestResult {
    let mut resource = patient();
    resource["maritalStatus"]["coding"][0]["code"] = json!("S");
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::PatternMismatch),
        ["Patient.maritalStatus"]
    );
    Ok(())
}

#[test]
fn a_value_holding_the_pattern_among_more_content_matches() -> TestResult {
    let mut resource = patient();
    resource["maritalStatus"] = json!({
        "coding": [
            {"system": "urn:example:local", "code": "x"},
            {"system": "http://terminology.hl7.org/CodeSystem/v3-MaritalStatus", "code": "M"}
        ],
        "text": "married"
    });
    let report = against_patient(&resource)?;
    assert!(
        locations(&report, MessageId::PatternMismatch).is_empty(),
        "{:#?}",
        report.issues()
    );
    Ok(())
}
