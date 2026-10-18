// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Slicing in an STU3 profile: STU3 states a discriminator as a type and a
//! path (<https://hl7.org/fhir/STU3/profiling.html#slicing>), which the walk
//! evaluates as it does an R4 one.

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

/// The system of the one identifier slice.
const LOCAL: &str = "http://example.org/fhir/identifiers/local";

/// Validates `resource` against an STU3 Patient profile whose identifiers
/// are sliced closed by `system`, with one required slice `local`.
fn against_sliced(resource: &serde_json::Value) -> TestResult<Report> {
    let definition = stu3_profile(
        "stu3-sliced",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.identifier", "path": "Patient.identifier", "min": 0, "max": "*",
             "slicing": {"discriminator": [{"type": "value", "path": "system"}], "rules": "closed"},
             "type": [{"code": "Identifier"}]},
            {"id": "Patient.identifier:local", "path": "Patient.identifier", "sliceName": "local",
             "min": 1, "max": "1", "type": [{"code": "Identifier"}]},
            {"id": "Patient.identifier:local.system", "path": "Patient.identifier.system",
             "min": 1, "max": "1", "type": [{"code": "uri"}], "fixedUri": LOCAL}
        ]),
    );
    let (_synthetic, set) = stu3_set(&[definition])?;
    validate_stu3(&set, resource, Some(&format!("{BASE}stu3-sliced")))
}

#[test]
fn an_item_matching_its_slice_raises_no_error() -> TestResult {
    let report = against_sliced(&json!({
        "resourceType": "Patient",
        "identifier": [{"system": LOCAL, "value": "1"}]
    }))?;
    assert!(errors(&report).is_empty(), "{:#?}", errors(&report));
    Ok(())
}

#[test]
fn an_item_in_no_slice_of_a_closed_slicing_is_an_error() -> TestResult {
    let report = against_sliced(&json!({
        "resourceType": "Patient",
        "identifier": [
            {"system": LOCAL, "value": "1"},
            {"system": "http://example.org/fhir/identifiers/other", "value": "2"}
        ]
    }))?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Patient.identifier[1]"]
    );
    Ok(())
}

#[test]
fn a_required_slice_left_empty_is_below_its_minimum() -> TestResult {
    let report = against_sliced(&json!({"resourceType": "Patient", "active": true}))?;
    assert_eq!(locations(&report, MessageId::MinCardinality), ["Patient"]);
    Ok(())
}
