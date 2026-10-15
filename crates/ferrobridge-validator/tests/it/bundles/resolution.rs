// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Each rule of bundle.html §2.36.4.1, observed through the core target
//! profiles of `Observation.subject` (Patient, Group, Device, Location): a
//! reference that resolves is held to them, one that does not is listed.

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use ferrobridge_validator::set::PackageSet;
use serde_json::json;

use super::bundle;
use super::observation;
use super::organization;
use super::patient;
use super::target_reasons;
use crate::support::TestResult;
use crate::support::core;
use crate::support::locations;
use crate::support::validate;

/// The fullUrl of the Observation entry.
const OBSERVATION: &str = "http://example.org/fhir/Observation/o1";

/// Validates `resource` against the core definitions.
fn core_report(resource: &serde_json::Value) -> TestResult<Report> {
    let set = PackageSet::load([core()])?;
    validate(&set, resource, None)
}

/// Answers whether the subject of the Observation entry resolved to a
/// resource its target profiles accept: nothing listed, nothing refused.
fn subject_accepted(report: &Report) -> bool {
    target_reasons(report, "Observation.subject").is_empty()
        && report
            .with_message(MessageId::TargetProfileMismatch)
            .next()
            .is_none()
}

#[test]
fn an_absolute_reference_matches_the_entry_with_that_full_url() -> TestResult {
    let reference = "http://example.org/fhir/Patient/p1";
    let report = core_report(&bundle(
        "collection",
        &[
            (OBSERVATION, observation(reference)),
            (reference, patient()),
        ],
    ))?;
    assert!(subject_accepted(&report), "{report:#?}");
    let report = core_report(&bundle(
        "collection",
        &[
            (OBSERVATION, observation(reference)),
            (reference, organization()),
        ],
    ))?;
    assert_eq!(
        locations(&report, MessageId::TargetProfileMismatch),
        ["Bundle.entry[0].resource.subject"]
    );
    Ok(())
}

#[test]
fn a_relative_reference_resolves_against_the_root_of_its_entrys_full_url() -> TestResult {
    let entries = |observation_url: &'static str| {
        bundle(
            "collection",
            &[
                (observation_url, observation("Patient/p1")),
                ("http://example.org/fhir/Patient/p1", patient()),
                ("http://other.example/fhir/Patient/p1", organization()),
            ],
        )
    };
    let report = core_report(&entries(OBSERVATION))?;
    assert!(subject_accepted(&report), "{report:#?}");
    let report = core_report(&entries("http://other.example/fhir/Observation/o1"))?;
    assert_eq!(
        locations(&report, MessageId::TargetProfileMismatch),
        ["Bundle.entry[0].resource.subject"],
        "the root of the other fullUrl names the Organization"
    );
    Ok(())
}

#[test]
fn a_relative_reference_from_an_entry_without_a_restful_full_url_has_no_meaning() -> TestResult {
    let report = core_report(&bundle(
        "collection",
        &[
            (
                "urn:uuid:4a1e8f9e-0000-4000-8000-000000000001",
                observation("Patient/p1"),
            ),
            ("http://example.org/fhir/Patient/p1", patient()),
        ],
    ))?;
    assert_eq!(
        target_reasons(&report, "Observation.subject"),
        [Reason::ReferenceOutOfScope]
    );
    Ok(())
}

#[test]
fn a_urn_uuid_reference_matches_its_entry_and_a_missing_one_is_unresolved() -> TestResult {
    let id = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000002";
    let report = core_report(&bundle(
        "collection",
        &[(OBSERVATION, observation(id)), (id, patient())],
    ))?;
    assert!(subject_accepted(&report), "{report:#?}");
    let report = core_report(&bundle(
        "collection",
        &[
            (
                OBSERVATION,
                observation("urn:uuid:4a1e8f9e-0000-4000-8000-000000000003"),
            ),
            (id, patient()),
        ],
    ))?;
    assert_eq!(
        target_reasons(&report, "Observation.subject"),
        [Reason::UnresolvedReference]
    );
    Ok(())
}

/// A history Bundle holding two versions of the Patient `p1`: version 1
/// carries a gender outside its required value set, and version 2 conforms.
fn two_versions(reference: &str) -> serde_json::Value {
    let url = "http://example.org/fhir/Patient/p1";
    let mut first = patient();
    first["meta"] = json!({"versionId": "1"});
    first["gender"] = json!("not-a-gender");
    let mut second = patient();
    second["meta"] = json!({"versionId": "2"});
    bundle(
        "history",
        &[
            (OBSERVATION, observation(reference)),
            (url, first),
            (url, second),
        ],
    )
}

#[test]
fn a_version_specific_reference_matches_the_meta_version_id() -> TestResult {
    let report = core_report(&two_versions("Patient/p1/_history/2"))?;
    assert!(subject_accepted(&report), "{report:#?}");
    let report = core_report(&two_versions("Patient/p1/_history/1"))?;
    assert_eq!(
        locations(&report, MessageId::TargetProfileMismatch),
        ["Bundle.entry[0].resource.subject"]
    );
    let report = core_report(&two_versions("Patient/p1/_history/3"))?;
    assert_eq!(
        target_reasons(&report, "Observation.subject"),
        [Reason::ReferenceOutOfScope],
        "an http version the Bundle lacks may be on the server"
    );
    Ok(())
}

#[test]
fn an_unversioned_reference_to_two_entries_is_an_ambiguous_error() -> TestResult {
    let report = core_report(&two_versions("Patient/p1"))?;
    assert_eq!(
        locations(&report, MessageId::AmbiguousReference),
        ["Bundle.entry[0].resource.subject"]
    );
    assert!(!report.is_valid());
    assert_eq!(
        target_reasons(&report, "Observation.subject"),
        [Reason::AmbiguousReference]
    );
    Ok(())
}

#[test]
fn a_local_reference_inside_an_entry_resolves_to_its_contained_resource() -> TestResult {
    let mut resource = observation("#p");
    let mut contained = patient();
    contained["id"] = json!("p");
    resource["contained"] = json!([contained]);
    let report = core_report(&bundle("collection", &[(OBSERVATION, resource)]))?;
    assert!(subject_accepted(&report), "{report:#?}");
    Ok(())
}
