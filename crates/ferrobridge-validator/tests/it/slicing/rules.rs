// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The slicing rules, the order, the `@default` slice, re-slicing and the
//! cardinality of slices and of the slicing entry
//! (<https://hl7.org/fhir/R4/profiling.html#slicing>, §5.1.0.10 to
//! §5.1.0.14; <https://hl7.org/fhir/R4/elementdefinition.html#slicing>,
//! §2.30.0.4.1).

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use serde_json::json;

use super::identifier_child;
use super::identifier_profile;
use super::identifier_slice;
use super::patient_with;
use super::system_slice;
use crate::support::TestResult;
use crate::support::locations;

/// Two slices of `Patient.identifier`, `a` before `b`, by their systems.
fn two_slices() -> Vec<serde_json::Value> {
    let mut slices = system_slice("a", "urn:a", "*");
    slices.extend(system_slice("b", "urn:b", "*"));
    slices
}

/// A slicing of `Patient.identifier` by system, with `rules` and `ordered`.
fn by_system(rules: &str, ordered: bool) -> serde_json::Value {
    json!({"discriminator": [{"type": "value", "path": "system"}], "rules": rules, "ordered": ordered})
}

/// The messages of the issues of `report` carrying `id`.
fn messages(report: &ferrobridge_validator::report::Report, id: MessageId) -> Vec<String> {
    report
        .with_message(id)
        .map(|issue| issue.message.clone())
        .collect()
}

#[test]
fn an_open_slicing_accepts_an_item_in_no_slice() -> TestResult {
    let definition = identifier_profile(&by_system("open", false), &two_slices());
    let report = patient_with(&[definition], &json!([{"system": "urn:other"}]))?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn an_item_in_no_slice_before_a_sliced_one_violates_open_at_end() -> TestResult {
    let definition = identifier_profile(&by_system("openAtEnd", true), &two_slices());
    let report = patient_with(
        std::slice::from_ref(&definition),
        &json!([{"system": "urn:other"}, {"system": "urn:a"}]),
    )?;
    assert_eq!(
        locations(&report, MessageId::OpenAtEnd),
        ["Patient.identifier[1]"]
    );
    let report = patient_with(
        &[definition],
        &json!([{"system": "urn:a"}, {"system": "urn:other"}]),
    )?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn an_ordered_slicing_refuses_items_out_of_slice_order() -> TestResult {
    let definition = identifier_profile(&by_system("open", true), &two_slices());
    let report = patient_with(
        &[definition],
        &json!([{"system": "urn:b"}, {"system": "urn:a"}]),
    )?;
    assert_eq!(
        locations(&report, MessageId::SliceOrder),
        ["Patient.identifier[1]"]
    );
    Ok(())
}

#[test]
fn an_item_in_no_slice_may_sit_between_ordered_open_slices() -> TestResult {
    let definition = identifier_profile(&by_system("open", true), &two_slices());
    let report = patient_with(
        &[definition],
        &json!([{"system": "urn:a"}, {"system": "urn:other"}, {"system": "urn:b"}]),
    )?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn the_default_slice_takes_an_item_in_no_other_slice_and_holds_it_to_its_constraints() -> TestResult
{
    let mut slices = system_slice("a", "urn:a", "*");
    slices.push(identifier_slice("@default", 0, "*"));
    slices.push(identifier_child("@default", "value", &json!({"min": 1})));
    let definition = identifier_profile(&by_system("closed", false), &slices);
    let report = patient_with(
        std::slice::from_ref(&definition),
        &json!([{"system": "urn:other", "value": "1"}]),
    )?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let report = patient_with(&[definition], &json!([{"system": "urn:other"}]))?;
    assert!(locations(&report, MessageId::UnmatchedSliceItem).is_empty());
    assert_eq!(
        locations(&report, MessageId::MinCardinality),
        ["Patient.identifier[0]"]
    );
    Ok(())
}

#[test]
fn a_default_slice_under_open_rules_is_named_and_not_used() -> TestResult {
    let mut slices = system_slice("a", "urn:a", "*");
    slices.push(identifier_slice("@default", 0, "*"));
    slices.push(identifier_child("@default", "value", &json!({"min": 1})));
    let definition = identifier_profile(&by_system("open", false), &slices);
    let report = patient_with(&[definition], &json!([{"system": "urn:other"}]))?;
    let entry = report
        .unevaluated()
        .iter()
        .find(|entry| entry.constraint == Constraint::Slicing)
        .expect("the default slice is named");
    assert_eq!(entry.reason, Reason::DefaultSliceNotClosed);
    assert!(
        report.is_valid(),
        "the item is held to no default slice: {:#?}",
        report.issues()
    );
    Ok(())
}

#[test]
fn a_reslice_counts_the_items_of_its_slice() -> TestResult {
    let mut slices = vec![
        json!({"id": "Patient.identifier:a", "path": "Patient.identifier", "sliceName": "a",
               "min": 0, "max": "*", "type": [{"code": "Identifier"}],
               "slicing": {"discriminator": [{"type": "value", "path": "use"}], "rules": "open"}}),
        identifier_child("a", "system", &json!({"fixedUri": "urn:a"})),
    ];
    slices.push(json!({"id": "Patient.identifier:a/official", "path": "Patient.identifier",
                       "sliceName": "a/official", "min": 0, "max": "1", "type": [{"code": "Identifier"}]}));
    slices.push(
        json!({"id": "Patient.identifier:a/official.use", "path": "Patient.identifier.use",
                       "min": 0, "max": "1", "fixedCode": "official"}),
    );
    let definition = identifier_profile(&by_system("open", false), &slices);
    let report = patient_with(
        &[definition],
        &json!([{"system": "urn:a", "use": "official"}, {"system": "urn:a", "use": "official"},
                {"system": "urn:a", "use": "usual"}]),
    )?;
    assert_eq!(
        messages(&report, MessageId::MaxCardinality),
        ["Patient.identifier:a/official occurs 2 times, above its maximum of 1"]
    );
    Ok(())
}

#[test]
fn a_slice_holds_at_most_its_maximum() -> TestResult {
    let definition =
        identifier_profile(&by_system("open", false), &system_slice("a", "urn:a", "1"));
    let report = patient_with(
        &[definition],
        &json!([{"system": "urn:a"}, {"system": "urn:a"}]),
    )?;
    assert_eq!(
        messages(&report, MessageId::MaxCardinality),
        ["Patient.identifier:a occurs 2 times, above its maximum of 1"]
    );
    assert_eq!(
        locations(&report, MessageId::MaxCardinality),
        ["Patient.identifier"]
    );
    Ok(())
}

#[test]
fn a_slice_holds_at_least_its_minimum() -> TestResult {
    let mut slices = vec![identifier_slice("a", 1, "*")];
    slices.push(identifier_child(
        "a",
        "system",
        &json!({"fixedUri": "urn:a"}),
    ));
    let definition = identifier_profile(&by_system("open", false), &slices);
    let report = patient_with(&[definition], &json!([{"system": "urn:b"}]))?;
    assert_eq!(
        messages(&report, MessageId::MinCardinality),
        ["Patient.identifier:a occurs 0 times, below its minimum of 1"]
    );
    assert_eq!(locations(&report, MessageId::MinCardinality), ["Patient"]);
    Ok(())
}

#[test]
fn the_minimum_on_the_slicing_entry_counts_every_item() -> TestResult {
    let mut definition =
        identifier_profile(&by_system("open", false), &system_slice("a", "urn:a", "*"));
    definition["snapshot"]["element"][1]["min"] = json!(2);
    let report = patient_with(
        std::slice::from_ref(&definition),
        &json!([{"system": "urn:a"}]),
    )?;
    assert_eq!(
        messages(&report, MessageId::MinCardinality),
        ["Patient.identifier occurs 1 times, below its minimum of 2"]
    );
    let report = patient_with(
        &[definition],
        &json!([{"system": "urn:a"}, {"system": "urn:other"}]),
    )?;
    assert!(
        report.is_valid(),
        "an item in no slice counts toward the entry: {:#?}",
        report.issues()
    );
    Ok(())
}
