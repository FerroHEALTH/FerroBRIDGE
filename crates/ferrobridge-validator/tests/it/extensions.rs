// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Extensions resolved by `url` and walked against their definition
//! (<https://hl7.org/fhir/R4/extensibility.html> §2.5.0.1 to §2.5.0.3).

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::issue::Severity;
use serde_json::json;

use crate::support::BASE;
use crate::support::TestResult;
use crate::support::against_patient;
use crate::support::issues;
use crate::support::locations;
use crate::support::patient;

#[test]
fn a_known_extension_with_the_type_it_defines_raises_no_error() -> TestResult {
    let mut resource = patient();
    resource["extension"] = json!([{"url": format!("{BASE}flag"), "valueBoolean": true}]);
    let report = against_patient(&resource)?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn an_extension_url_with_a_fragment_is_an_unknown_extension_and_the_walk_goes_on() -> TestResult {
    // extensibility.html §2.5.0.3: an extension the processor does not know is ignored; a
    // fragment names no StructureDefinition the package set resolves.
    let mut resource = patient();
    resource["extension"] =
        json!([{"url": "http://example.org/fhir/extensions#part", "valueString": "x"}]);
    resource["gender"] = json!("male");
    let report = against_patient(&resource)?;
    let unknown = issues(&report, MessageId::UnknownExtension);
    assert_eq!(unknown.len(), 1, "{:#?}", report.issues());
    assert_eq!(unknown[0].expression, "Patient.extension[0]");
    assert_eq!(unknown[0].severity, Severity::Warning);
    assert_eq!(
        locations(&report, MessageId::FixedMismatch),
        ["Patient.gender"],
        "the walk continues past the extension"
    );
    Ok(())
}

#[test]
fn a_modifier_extension_url_with_a_fragment_is_an_unknown_modifier_error() -> TestResult {
    // extensibility.html §2.5.0.2: an unknown modifier extension is not processed.
    let mut resource = patient();
    resource["modifierExtension"] =
        json!([{"url": "http://example.org/fhir/extensions#stop", "valueBoolean": true}]);
    let report = against_patient(&resource)?;
    let unknown = issues(&report, MessageId::UnknownModifierExtension);
    assert_eq!(unknown.len(), 1, "{:#?}", report.issues());
    assert_eq!(unknown[0].expression, "Patient.modifierExtension[0]");
    assert_eq!(unknown[0].severity, Severity::Error);
    Ok(())
}

#[test]
fn an_extension_value_of_a_type_its_definition_does_not_allow_is_refused() -> TestResult {
    let mut resource = patient();
    resource["extension"] = json!([{"url": format!("{BASE}flag"), "valueString": "yes"}]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::ChoiceTypeNotAllowed),
        ["Patient.extension[0].value.ofType(string)"]
    );
    Ok(())
}

#[test]
fn an_extension_missing_the_value_its_definition_requires_is_refused() -> TestResult {
    let mut resource = patient();
    resource["extension"] = json!([{"url": format!("{BASE}flag"),
        "extension": [{"url": format!("{BASE}label"), "valueString": "x"}]}]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::MinCardinality),
        ["Patient.extension[0]"]
    );
    assert_eq!(
        locations(&report, MessageId::MaxCardinality),
        ["Patient.extension[0].extension"]
    );
    Ok(())
}

#[test]
fn an_extension_on_a_primitive_is_resolved_too() -> TestResult {
    let mut resource = patient();
    resource["_gender"] = json!({"extension": [{"url": format!("{BASE}flag"), "valueCode": "x"}]});
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::ChoiceTypeNotAllowed),
        ["Patient.gender.extension[0].value.ofType(code)"]
    );
    Ok(())
}

#[test]
fn an_unknown_extension_is_a_warning() -> TestResult {
    let mut resource = patient();
    resource["extension"] = json!([{"url": "http://example.org/unknown", "valueBoolean": true}]);
    let report = against_patient(&resource)?;
    let found = issues(&report, MessageId::UnknownExtension);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(found[0].expression, "Patient.extension[0]");
    assert!(report.is_valid(), "a warning leaves the instance valid");
    Ok(())
}

#[test]
fn an_unknown_modifier_extension_is_an_error() -> TestResult {
    let mut resource = patient();
    resource["modifierExtension"] =
        json!([{"url": "http://example.org/unknown", "valueBoolean": true}]);
    let report = against_patient(&resource)?;
    let found = issues(&report, MessageId::UnknownModifierExtension);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity, Severity::Error);
    assert_eq!(found[0].expression, "Patient.modifierExtension[0]");
    Ok(())
}

#[test]
fn a_known_modifier_in_modifier_extension_raises_no_error() -> TestResult {
    let mut resource = patient();
    resource["modifierExtension"] = json!([{"url": format!("{BASE}stop"), "valueBoolean": true}]);
    let report = against_patient(&resource)?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn a_modifier_carried_in_extension_is_refused() -> TestResult {
    let mut resource = patient();
    resource["extension"] = json!([{"url": format!("{BASE}stop"), "valueBoolean": true}]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::ModifierMismatch),
        ["Patient.extension[0]"]
    );
    Ok(())
}

#[test]
fn a_relative_url_outside_a_complex_extension_is_refused() -> TestResult {
    let mut resource = patient();
    resource["extension"] = json!([{"url": "flag", "valueBoolean": true}]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::RelativeExtensionUrl),
        ["Patient.extension[0]"]
    );
    Ok(())
}

#[test]
fn a_url_naming_a_definition_of_another_type_is_refused() -> TestResult {
    let mut resource = patient();
    resource["extension"] =
        json!([{"url": "http://hl7.org/fhir/StructureDefinition/Patient", "valueBoolean": true}]);
    let report = against_patient(&resource)?;
    assert_eq!(
        locations(&report, MessageId::NotAnExtension),
        ["Patient.extension[0]"]
    );
    Ok(())
}
