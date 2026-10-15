// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every entry's profiles: the profile its slice names (`type.profile`, "the
//! content must conform to at least one of them") and every profile it
//! claims in `meta.profile`, which R4 leaves to policy and this validator
//! checks.

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use serde_json::json;

use super::bundle;
use crate::support::BASE;
use crate::support::TestResult;
use crate::support::locations;
use crate::support::patient;
use crate::support::patient_profile;
use crate::support::patient_url;
use crate::support::profile;
use crate::support::set_with;
use crate::support::validate;

/// A collection Bundle profile whose entries are sliced by the type of
/// their resource, the slice `patient` holding a Patient of the synthetic
/// Patient profile.
fn bundle_profile() -> serde_json::Value {
    profile(
        "bundle-test",
        "Bundle",
        &json!([
            {"id": "Bundle", "path": "Bundle", "min": 0, "max": "*"},
            {"id": "Bundle.entry", "path": "Bundle.entry", "min": 0, "max": "*",
             "slicing": {"discriminator": [{"type": "type", "path": "resource"}], "rules": "open"}},
            {"id": "Bundle.entry:patient", "path": "Bundle.entry", "sliceName": "patient",
             "min": 0, "max": "*"},
            {"id": "Bundle.entry:patient.resource", "path": "Bundle.entry.resource",
             "min": 1, "max": "1", "type": [{"code": "Patient", "profile": [patient_url()]}]}
        ]),
    )
}

/// A synthetic Patient breaking the synthetic Patient profile: its family
/// name is longer than five characters.
fn long_named() -> serde_json::Value {
    let mut resource = patient();
    resource["name"] = json!([{"family": "Doe-Smith"}]);
    resource
}

#[test]
fn an_entry_is_held_to_the_profile_its_slice_names() -> TestResult {
    let (_synthetic, set) = set_with(&[bundle_profile(), patient_profile()])?;
    let url = format!("{BASE}bundle-test");
    let entries = |resource| {
        bundle(
            "collection",
            &[("urn:uuid:4a1e8f9e-0000-4000-8000-000000000020", resource)],
        )
    };
    let report = validate(&set, &entries(patient()), Some(&url))?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let report = validate(&set, &entries(long_named()), Some(&url))?;
    assert_eq!(
        locations(&report, MessageId::MaxLength),
        ["Bundle.entry[0].resource.name[0].family"]
    );
    Ok(())
}

/// A collection Bundle holding `resource`, claiming `claims`.
fn claiming(mut resource: serde_json::Value, claims: &[String]) -> serde_json::Value {
    resource["meta"] = json!({"profile": claims});
    bundle(
        "collection",
        &[("urn:uuid:4a1e8f9e-0000-4000-8000-000000000021", resource)],
    )
}

#[test]
fn an_entry_conforming_to_the_profile_it_claims_passes() -> TestResult {
    let (_synthetic, set) = set_with(&[patient_profile()])?;
    let report = validate(&set, &claiming(patient(), &[patient_url()]), None)?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    assert!(
        report
            .unevaluated()
            .iter()
            .all(|entry| !matches!(entry.constraint, Constraint::ClaimedProfile { .. }))
    );
    Ok(())
}

#[test]
fn an_entry_breaking_the_profile_it_claims_is_refused_naming_it() -> TestResult {
    let (_synthetic, set) = set_with(&[patient_profile()])?;
    let report = validate(&set, &claiming(long_named(), &[patient_url()]), None)?;
    assert_eq!(
        locations(&report, MessageId::ClaimedProfileMismatch),
        ["Bundle.entry[0].resource"]
    );
    assert_eq!(
        locations(&report, MessageId::MaxLength),
        ["Bundle.entry[0].resource.name[0].family"],
        "the claim's own findings join the report once"
    );
    Ok(())
}

#[test]
fn an_entry_claiming_a_profile_the_set_lacks_is_listed() -> TestResult {
    let (_synthetic, set) = set_with(&[patient_profile()])?;
    let absent = format!("{BASE}absent");
    let report = validate(
        &set,
        &claiming(patient(), std::slice::from_ref(&absent)),
        None,
    )?;
    let listed: Vec<_> = report
        .unevaluated()
        .iter()
        .filter_map(|entry| match &entry.constraint {
            Constraint::ClaimedProfile { url } => Some((url.clone(), entry.reason.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(listed, [(absent, Reason::ProfileNotFound)]);
    Ok(())
}
