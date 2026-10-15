// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Extension slices: "extensions are always sliced by the url element"
//! (<https://hl7.org/fhir/R4/profiling.html#discriminator>, §5.1.0.11), a
//! slice taking its definition from `type.profile`, and the children of a
//! complex extension matched by their relative urls
//! (<https://hl7.org/fhir/R4/extensibility.html>, §2.5.0.1).

use ferrobridge_validator::issue::MessageId;
use serde_json::json;

use super::against;
use crate::support::BASE;
use crate::support::TestResult;
use crate::support::extension;
use crate::support::locations;
use crate::support::profile;

/// A Patient profile slicing `extension` by url, with a slice `flag` of at
/// least one taking its definition from the extension `flag`.
fn flagged_patient() -> serde_json::Value {
    profile(
        "flagged-patient",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.extension", "path": "Patient.extension", "min": 0, "max": "*",
             "slicing": {"discriminator": [{"type": "value", "path": "url"}], "rules": "open"}},
            {"id": "Patient.extension:flag", "path": "Patient.extension", "sliceName": "flag",
             "min": 1, "max": "1", "type": [{"code": "Extension", "profile": [format!("{BASE}flag")]}]}
        ]),
    )
}

#[test]
fn an_extension_slice_matches_by_the_url_its_profile_fixes() -> TestResult {
    let url = format!("{BASE}flagged-patient");
    let set = [flagged_patient(), extension("flag", "boolean", false)];
    let resource = json!({"resourceType": "Patient",
                          "extension": [{"url": format!("{BASE}flag"), "valueBoolean": true}]});
    let report = against(&set, &resource, &url)?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let resource = json!({"resourceType": "Patient"});
    let report = against(&set, &resource, &url)?;
    let messages: Vec<_> = report
        .with_message(MessageId::MinCardinality)
        .map(|issue| issue.message.clone())
        .collect();
    assert_eq!(
        messages,
        ["Patient.extension:flag occurs 0 times, below its minimum of 1"]
    );
    Ok(())
}

/// A complex extension `complex` whose children are sliced closed by url,
/// with one child `part` whose value is a string.
fn complex_extension() -> serde_json::Value {
    let mut definition = profile(
        "complex",
        "Extension",
        &json!([
            {"id": "Extension", "path": "Extension", "min": 0, "max": "*"},
            {"id": "Extension.extension", "path": "Extension.extension", "min": 1, "max": "*",
             "slicing": {"discriminator": [{"type": "value", "path": "url"}], "rules": "closed"}},
            {"id": "Extension.extension:part", "path": "Extension.extension", "sliceName": "part",
             "min": 1, "max": "1"},
            {"id": "Extension.extension:part.extension", "path": "Extension.extension.extension",
             "min": 0, "max": "0"},
            {"id": "Extension.extension:part.url", "path": "Extension.extension.url",
             "min": 1, "max": "1", "type": [{"code": "uri"}], "fixedUri": "part"},
            {"id": "Extension.extension:part.value[x]", "path": "Extension.extension.value[x]",
             "min": 1, "max": "1", "type": [{"code": "string"}]},
            {"id": "Extension.url", "path": "Extension.url", "min": 1, "max": "1",
             "type": [{"code": "uri"}], "fixedUri": format!("{BASE}complex")},
            {"id": "Extension.value[x]", "path": "Extension.value[x]", "min": 0, "max": "0"}
        ]),
    );
    definition["context"] = json!([{"type": "element", "expression": "Element"}]);
    definition
}

/// A Patient carrying the complex extension with the children `children`.
fn patient_with_complex(children: &serde_json::Value) -> serde_json::Value {
    json!({"resourceType": "Patient",
           "extension": [{"url": format!("{BASE}complex"), "extension": children}]})
}

#[test]
fn a_child_of_a_complex_extension_is_matched_by_its_relative_url() -> TestResult {
    let url = "http://hl7.org/fhir/StructureDefinition/Patient";
    let report = against(
        &[complex_extension()],
        &patient_with_complex(&json!([{"url": "part", "valueString": "x"}])),
        url,
    )?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    assert!(locations(&report, MessageId::RelativeExtensionUrl).is_empty());
    Ok(())
}

#[test]
fn a_child_of_a_complex_extension_is_held_to_its_slice() -> TestResult {
    let url = "http://hl7.org/fhir/StructureDefinition/Patient";
    let report = against(
        &[complex_extension()],
        &patient_with_complex(&json!([{"url": "part", "valueBoolean": true}])),
        url,
    )?;
    assert_eq!(
        locations(&report, MessageId::ChoiceTypeNotAllowed),
        ["Patient.extension[0].extension[0].value.ofType(boolean)"]
    );
    Ok(())
}

#[test]
fn a_child_url_the_complex_extension_does_not_slice_is_refused_by_its_closed_rule() -> TestResult {
    let url = "http://hl7.org/fhir/StructureDefinition/Patient";
    let report = against(
        &[complex_extension()],
        &patient_with_complex(&json!([{"url": "part", "valueString": "x"},
                                      {"url": "other", "valueString": "y"}])),
        url,
    )?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Patient.extension[0].extension[1]"]
    );
    Ok(())
}
