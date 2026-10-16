// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The functions an invariant calls outside the document (R4 fhirpath.html
//! §2.9.1.5): `resolve()` through the validator's reference resolution,
//! `memberOf()` through the value sets of the package set and `conformsTo()`
//! through a validation of its own, each answered when the run can, and the
//! invariant listed with the reason when it cannot.

use ferrobridge_validator::report::InvariantStage;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use serde_json::json;

use super::failed_at;
use super::listed;
use crate::support::BASE;
use crate::support::TestResult;
use crate::support::profile;
use crate::support::set_with;
use crate::support::validate;

/// A synthetic rule `key` over `expression`, declared by `structure`.
fn rule(structure: &str, key: &str, expression: &str) -> serde_json::Value {
    json!({"key": key, "severity": "error", "human": format!("the synthetic rule {key}"),
           "expression": expression, "source": format!("{BASE}{structure}")})
}

/// An Observation profile whose subject resolves to a Patient.
fn observation_profile() -> serde_json::Value {
    profile(
        "observation-subject",
        "Observation",
        &json!([{"id": "Observation", "path": "Observation", "min": 0, "max": "*", "constraint": [
            rule("observation-subject", "test-subject", "subject.resolve().is(Patient)")
        ]}]),
    )
}

/// A Bundle profile whose Observation's subject resolves to a Patient, read
/// from the Bundle element.
fn bundle_profile() -> serde_json::Value {
    profile(
        "bundle-subject",
        "Bundle",
        &json!([{"id": "Bundle", "path": "Bundle", "min": 0, "max": "*", "constraint": [
            rule("bundle-subject", "test-bundle-subject",
                 "entry.resource.ofType(Observation).subject.resolve().is(Patient)")
        ]}]),
    )
}

/// A collection Bundle holding an Observation claiming
/// `observation-subject` and an entry of type `subject_type` it references.
fn bundle(subject_type: &str) -> serde_json::Value {
    let subject = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000060";
    json!({"resourceType": "Bundle", "type": "collection", "entry": [
        {"fullUrl": "urn:uuid:4a1e8f9e-0000-4000-8000-000000000061", "resource": {
            "resourceType": "Observation",
            "meta": {"profile": [format!("{BASE}observation-subject")]},
            "status": "final", "code": {"text": "synthetic"}, "subject": {"reference": subject}}},
        {"fullUrl": subject, "resource": {"resourceType": subject_type, "name": [{"family": "Doe"}]}}
    ]})
}

/// Validates `resource` against `profile` in the set of the seam profiles.
fn checked(resource: &serde_json::Value, profile: Option<&str>) -> TestResult<Report> {
    let (_synthetic, set) = set_with(&[
        observation_profile(),
        bundle_profile(),
        patient_profile(),
        named_profile(),
        female_only(),
    ])?;
    let profile = profile.map(|id| format!("{BASE}{id}"));
    validate(&set, resource, profile.as_deref())
}

#[test]
fn resolve_reaches_the_entry_a_reference_names() -> TestResult {
    let report = checked(&bundle("Patient"), None)?;
    assert!(
        failed_at(&report, "test-subject").is_empty(),
        "{:#?}",
        report.issues()
    );
    assert!(
        listed(&report, "test-subject").is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    let report = checked(&bundle("Practitioner"), None)?;
    assert_eq!(
        failed_at(&report, "test-subject"),
        ["Bundle.entry[0].resource"]
    );
    Ok(())
}

#[test]
fn resolve_from_the_bundle_element_reaches_its_entries() -> TestResult {
    let report = checked(&bundle("Patient"), Some("bundle-subject"))?;
    assert!(
        failed_at(&report, "test-bundle-subject").is_empty(),
        "{:#?}",
        report.issues()
    );
    assert!(
        listed(&report, "test-bundle-subject").is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    let report = checked(&bundle("Practitioner"), Some("bundle-subject"))?;
    assert_eq!(failed_at(&report, "test-bundle-subject"), ["Bundle"]);
    Ok(())
}

#[test]
fn resolve_outside_the_resources_held_is_listed() -> TestResult {
    let observation = json!({"resourceType": "Observation", "status": "final",
        "code": {"text": "synthetic"}, "subject": {"reference": "Patient/p1"}});
    let report = checked(&observation, Some("observation-subject"))?;
    assert!(failed_at(&report, "test-subject").is_empty());
    let entries = listed(&report, "test-subject");
    assert_eq!(entries.len(), 1, "{:#?}", report.unevaluated());
    assert!(
        matches!(&entries[0].reason, Reason::InvariantRefused { stage: InvariantStage::Evaluate, error, .. }
            if error.contains("outside the resources searched")),
        "{:#?}",
        entries[0].reason
    );
    Ok(())
}

/// A value set holding the administrative gender `female` alone.
fn female_only() -> serde_json::Value {
    json!({
        "resourceType": "ValueSet", "id": "female-only",
        "url": "http://example.org/fhir/ValueSet/female-only", "version": "0.1.0", "status": "active",
        "compose": {"include": [{"system": "http://hl7.org/fhir/administrative-gender",
                                 "concept": [{"code": "female"}]}]}
    })
}

/// A Patient profile with a name.
fn named_profile() -> serde_json::Value {
    profile(
        "patient-named",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.name", "path": "Patient.name", "min": 1, "max": "*", "type": [{"code": "HumanName"}]}
        ]),
    )
}

/// A Patient profile whose rules ask for a gender from `female-only`, from a
/// value set the set lacks, conformance to `patient-named` and conformance
/// to a profile the set lacks.
fn patient_profile() -> serde_json::Value {
    profile(
        "patient-seams",
        "Patient",
        &json!([{"id": "Patient", "path": "Patient", "min": 0, "max": "*", "constraint": [
            rule("patient-seams", "test-member",
                 "gender.memberOf('http://example.org/fhir/ValueSet/female-only')"),
            rule("patient-seams", "test-absent-set",
                 "gender.memberOf('http://example.org/fhir/ValueSet/absent')"),
            rule("patient-seams", "test-conforms",
                 &format!("conformsTo('{BASE}patient-named')")),
            rule("patient-seams", "test-absent-profile",
                 "conformsTo('http://example.org/fhir/StructureDefinition/absent')")
        ]}]),
    )
}

#[test]
fn member_of_reads_a_value_set_the_package_set_carries() -> TestResult {
    let female =
        json!({"resourceType": "Patient", "gender": "female", "name": [{"family": "Doe"}]});
    let report = checked(&female, Some("patient-seams"))?;
    assert!(
        failed_at(&report, "test-member").is_empty(),
        "{:#?}",
        report.issues()
    );
    assert!(
        listed(&report, "test-member").is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    let male = json!({"resourceType": "Patient", "gender": "male", "name": [{"family": "Doe"}]});
    let report = checked(&male, Some("patient-seams"))?;
    assert_eq!(failed_at(&report, "test-member"), ["Patient"]);
    Ok(())
}

#[test]
fn member_of_a_value_set_the_package_set_lacks_is_listed() -> TestResult {
    let female = json!({"resourceType": "Patient", "gender": "female"});
    let report = checked(&female, Some("patient-seams"))?;
    assert!(failed_at(&report, "test-absent-set").is_empty());
    let entries = listed(&report, "test-absent-set");
    assert_eq!(entries.len(), 1, "{:#?}", report.unevaluated());
    assert!(
        matches!(&entries[0].reason, Reason::InvariantRefused { error, .. }
            if error.contains("http://example.org/fhir/ValueSet/absent")),
        "{:#?}",
        entries[0].reason
    );
    Ok(())
}

#[test]
fn conforms_to_validates_against_a_profile_of_the_package_set() -> TestResult {
    let named = json!({"resourceType": "Patient", "name": [{"family": "Doe"}]});
    assert!(failed_at(&checked(&named, Some("patient-seams"))?, "test-conforms").is_empty());
    let unnamed = json!({"resourceType": "Patient", "gender": "female"});
    let report = checked(&unnamed, Some("patient-seams"))?;
    assert_eq!(failed_at(&report, "test-conforms"), ["Patient"]);
    let entries = listed(&report, "test-absent-profile");
    assert_eq!(entries.len(), 1, "{:#?}", report.unevaluated());
    assert!(
        matches!(&entries[0].reason, Reason::InvariantRefused { error, .. }
            if error.contains("not in the package set")),
        "{:#?}",
        entries[0].reason
    );
    Ok(())
}
