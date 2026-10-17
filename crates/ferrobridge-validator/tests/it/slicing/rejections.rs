// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reason an item is not in a slice that holds too few: when a
//! validation decides membership (a `profile` discriminator, or slicing
//! with no discriminator, profiling.html §5.1.0.11) and a slice falls short
//! of its minimum (§5.1.0.12), the errors of the rejected item closest to the
//! slice are named as information. No specification governs this: our own
//! design.

use ferrobridge_validator::issue::Issue;
use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::issue::Severity;
use ferrobridge_validator::report::Report;
use serde_json::json;

use super::against;
use super::identifier_child;
use super::identifier_profile;
use super::identifier_slice;
use super::patient_with;
use crate::support::BASE;
use crate::support::TestResult;
use crate::support::locations;
use crate::support::profile;

/// The identifier profile with no discriminator and the slice `official`
/// of `min..1`, whose system is fixed and whose value is required.
fn official(min: u32) -> Vec<serde_json::Value> {
    vec![identifier_profile(
        &json!({"rules": "open"}),
        &[
            identifier_slice("official", min, "1"),
            identifier_child(
                "official",
                "system",
                &json!({"min": 1, "fixedUri": "urn:official"}),
            ),
            identifier_child("official", "value", &json!({"min": 1})),
        ],
    )]
}

/// The slice-rejection issues of `report`.
fn rejections(report: &Report) -> Vec<&Issue> {
    report.with_message(MessageId::SliceRejection).collect()
}

#[test]
fn a_short_slice_names_why_the_item_without_a_discriminator_failed_it() -> TestResult {
    let report = patient_with(
        &official(1),
        &json!([{"system": "urn:other", "value": "1"}]),
    )?;
    assert_eq!(
        locations(&report, MessageId::MinCardinality),
        ["Patient"],
        "{:#?}",
        report.issues()
    );
    let found = rejections(&report);
    assert_eq!(found.len(), 1, "{:#?}", report.issues());
    assert_eq!(found[0].severity, Severity::Information);
    assert_eq!(found[0].expression, "Patient.identifier[0].system");
    assert!(
        found[0].message.contains("Patient.identifier:official"),
        "{}",
        found[0].message
    );
    Ok(())
}

#[test]
fn the_rejected_item_with_the_fewest_errors_is_the_one_named() -> TestResult {
    let report = patient_with(
        &official(1),
        &json!([{"system": "urn:other"}, {"system": "urn:official"}]),
    )?;
    let found = rejections(&report);
    assert_eq!(found.len(), 1, "{:#?}", report.issues());
    assert_eq!(found[0].expression, "Patient.identifier[1]");
    Ok(())
}

#[test]
fn a_slice_that_holds_enough_items_names_no_rejection() -> TestResult {
    let report = patient_with(
        &official(0),
        &json!([{"system": "urn:other", "value": "1"}]),
    )?;
    assert!(rejections(&report).is_empty(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn a_short_slice_names_why_the_target_failed_its_profile_discriminator() -> TestResult {
    let definition = profile(
        "named-gp-required",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.generalPractitioner", "path": "Patient.generalPractitioner",
             "min": 0, "max": "*",
             "slicing": {"discriminator": [{"type": "profile", "path": "resolve()"}], "rules": "open"}},
            {"id": "Patient.generalPractitioner:named", "path": "Patient.generalPractitioner",
             "sliceName": "named", "min": 1, "max": "*",
             "type": [{"code": "Reference", "targetProfile": [format!("{BASE}named-organization")]}]}
        ]),
    );
    let organization = profile(
        "named-organization",
        "Organization",
        &json!([
            {"id": "Organization", "path": "Organization", "min": 0, "max": "*"},
            {"id": "Organization.name", "path": "Organization.name", "min": 1, "max": "1",
             "type": [{"code": "string"}]}
        ]),
    );
    let resource = json!({"resourceType": "Patient",
        "contained": [{"resourceType": "Organization", "id": "o1", "active": true}],
        "generalPractitioner": [{"reference": "#o1"}]});
    let report = against(
        &[definition, organization],
        &resource,
        &format!("{BASE}named-gp-required"),
    )?;
    let found = rejections(&report);
    assert_eq!(found.len(), 1, "{:#?}", report.issues());
    assert!(
        found[0].message.contains("Patient.generalPractitioner[0]")
            && found[0].message.contains("Organization.name"),
        "{}",
        found[0].message
    );
    Ok(())
}
