// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `type.targetProfile`, any of which the target SHALL conform to, and
//! `type.aggregation` (elementdefinition.html §2.30.0.4.4: "the target of
//! the reference SHALL be aggregated as defined"), on the resolved target.

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use serde_json::json;

use super::bundle;
use super::observation;
use super::target_reasons;
use crate::support::BASE;
use crate::support::TestResult;
use crate::support::locations;
use crate::support::patient;
use crate::support::patient_profile;
use crate::support::profile;
use crate::support::set_with;
use crate::support::validate;

/// An Observation profile whose subject SHALL conform to one of `targets`
/// and be aggregated in one of `modes`.
fn observation_profile(targets: &[String], modes: &[&str]) -> serde_json::Value {
    let mut kind = json!({"code": "Reference"});
    if !targets.is_empty() {
        kind["targetProfile"] = json!(targets);
    }
    if !modes.is_empty() {
        kind["aggregation"] = json!(modes);
    }
    profile(
        "observation-test",
        "Observation",
        &json!([
            {"id": "Observation", "path": "Observation", "min": 0, "max": "*"},
            {"id": "Observation.subject", "path": "Observation.subject", "min": 1, "max": "1",
             "type": [kind]}
        ]),
    )
}

/// Validates `resource` against the Observation profile of `targets` and
/// `modes`, with the synthetic Patient profile beside it.
fn against(resource: &serde_json::Value, targets: &[String], modes: &[&str]) -> TestResult<Report> {
    let (_synthetic, set) = set_with(&[observation_profile(targets, modes), patient_profile()])?;
    validate(&set, resource, Some(&format!("{BASE}observation-test")))
}

/// An Observation holding `patient` as the contained resource `p` and
/// naming it as its subject.
fn with_contained(patient: serde_json::Value) -> serde_json::Value {
    let mut contained = patient;
    contained["id"] = json!("p");
    let mut resource = observation("#p");
    resource["contained"] = json!([contained]);
    resource
}

/// The canonical of the synthetic Patient profile.
fn patient_test() -> String {
    format!("{BASE}patient-test")
}

#[test]
fn a_target_conforming_to_its_target_profile_passes() -> TestResult {
    let report = against(&with_contained(patient()), &[patient_test()], &[])?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    assert!(target_reasons(&report, "Observation.subject").is_empty());
    Ok(())
}

#[test]
fn a_target_breaking_its_target_profile_is_refused_at_the_reference() -> TestResult {
    let mut unnamed = patient();
    unnamed.as_object_mut().expect("an object").remove("name");
    let report = against(&with_contained(unnamed), &[patient_test()], &[])?;
    assert_eq!(
        locations(&report, MessageId::TargetProfileMismatch),
        ["Observation.subject"]
    );
    Ok(())
}

#[test]
fn a_target_failing_one_profile_may_conform_to_another_the_set_lacks() -> TestResult {
    let mut unnamed = patient();
    unnamed.as_object_mut().expect("an object").remove("name");
    let absent = format!("{BASE}absent");
    let report = against(&with_contained(unnamed), &[patient_test(), absent], &[])?;
    assert!(
        locations(&report, MessageId::TargetProfileMismatch).is_empty(),
        "{:#?}",
        report.issues()
    );
    assert_eq!(
        target_reasons(&report, "Observation.subject"),
        [Reason::ProfileNotFound]
    );
    Ok(())
}

/// The modes the aggregation of `Observation.subject` was listed
/// unevaluated for.
fn aggregation_reasons(report: &Report) -> Vec<Reason> {
    report
        .unevaluated()
        .iter()
        .filter(|entry| matches!(entry.constraint, Constraint::Aggregation { .. }))
        .map(|entry| entry.reason.clone())
        .collect()
}

#[test]
fn a_contained_target_meets_the_contained_aggregation() -> TestResult {
    let report = against(&with_contained(patient()), &[], &["contained"])?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let report = against(&with_contained(patient()), &[], &["referenced"])?;
    assert_eq!(
        locations(&report, MessageId::AggregationMismatch),
        ["Observation.subject"]
    );
    Ok(())
}

#[test]
fn a_reference_without_a_literal_breaks_any_aggregation() -> TestResult {
    let mut resource = observation("#p");
    resource["subject"] = json!({"identifier": {"system": "urn:example:patients", "value": "1"}});
    let report = against(&resource, &[], &["referenced"])?;
    assert_eq!(
        locations(&report, MessageId::AggregationMismatch),
        ["Observation.subject"]
    );
    Ok(())
}

#[test]
fn the_bundled_aggregation_needs_the_target_in_the_bundle() -> TestResult {
    let url = format!("{BASE}observation-test");
    let claimed = |reference: &str| {
        let mut resource = observation(reference);
        resource["meta"] = json!({"profile": [url]});
        resource
    };
    let (_synthetic, set) = set_with(&[observation_profile(&[], &["bundled"]), patient_profile()])?;
    let patient_url = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000010";
    let found = bundle(
        "collection",
        &[
            (
                "urn:uuid:4a1e8f9e-0000-4000-8000-000000000011",
                claimed(patient_url),
            ),
            (patient_url, patient()),
        ],
    );
    let report = validate(&set, &found, None)?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let missing = bundle(
        "collection",
        &[(
            "urn:uuid:4a1e8f9e-0000-4000-8000-000000000011",
            claimed("urn:uuid:4a1e8f9e-0000-4000-8000-000000000012"),
        )],
    );
    let report = validate(&set, &missing, None)?;
    assert_eq!(
        locations(&report, MessageId::AggregationMismatch),
        ["Bundle.entry[0].resource.subject"]
    );
    let alone = validate(&set, &observation("Patient/p1"), Some(&url))?;
    assert!(alone.is_valid(), "{:#?}", alone.issues());
    assert_eq!(aggregation_reasons(&alone), [Reason::NoBundle]);
    Ok(())
}
