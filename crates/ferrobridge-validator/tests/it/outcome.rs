// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The report: every constraint met and not evaluated is named, and the
//! report converts into an R4 `OperationOutcome`
//! (<https://hl7.org/fhir/R4/operationoutcome.html>).

use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::MESSAGE_SYSTEM;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use ferrobridge_validator::set::PackageSet;
use fhir_types::r4::operation_outcome::OperationOutcome;
use serde_json::json;

use crate::support::BASE;
use crate::support::TestResult;
use crate::support::against_patient;
use crate::support::core;
use crate::support::patient;
use crate::support::patient_url;
use crate::support::validate;

#[test]
fn a_slice_without_a_value_at_its_discriminator_is_named_unevaluated() -> TestResult {
    let mut resource = patient();
    resource["identifier"] = json!([{"system": "urn:example:local", "value": "1"}]);
    let report = against_patient(&resource)?;
    let slicing: Vec<_> = report
        .unevaluated()
        .iter()
        .filter(|entry| matches!(entry.constraint, Constraint::Discriminator { .. }))
        .collect();
    assert_eq!(slicing.len(), 1, "{:#?}", report.unevaluated());
    assert_eq!(slicing[0].structure, patient_url());
    assert_eq!(slicing[0].element, "Patient.identifier");
    assert_eq!(slicing[0].location, "Patient.identifier[0]");
    assert_eq!(
        slicing[0].constraint,
        Constraint::Discriminator {
            kind: "value".to_owned(),
            path: "system".to_owned()
        }
    );
    assert_eq!(slicing[0].reason, Reason::NoDiscriminatorValue);
    Ok(())
}

#[test]
fn the_core_invariants_of_a_datatype_reached_are_named_unevaluated() -> TestResult {
    let set = PackageSet::load([core()])?;
    let report = validate(&set, &patient(), None)?;
    let invariant = report.unevaluated().iter().find(|entry| {
        entry.structure == "http://hl7.org/fhir/StructureDefinition/HumanName"
            && matches!(&entry.constraint, Constraint::Invariant { key, .. } if key == "ele-1")
    });
    let invariant = invariant.expect("ele-1 on HumanName is listed");
    assert_eq!(invariant.reason, Reason::Invariants);
    assert_eq!(invariant.location, "Patient.name[0]");
    Ok(())
}

#[test]
fn a_required_binding_reached_is_named_unevaluated() -> TestResult {
    let set = PackageSet::load([core()])?;
    let set_report = validate(
        &set,
        &json!({
        "resourceType": "Patient",
        "name": [{"family": "Doe", "use": "official"}],
        "gender": "female"
        }),
        None,
    )?;
    let binding = set_report.unevaluated().iter().find(|entry| {
        matches!(&entry.constraint, Constraint::Binding { strength, value_set }
            if strength == "required"
                && value_set.as_deref().is_some_and(|set| set.starts_with("http://hl7.org/fhir/ValueSet/name-use")))
    });
    assert!(binding.is_some(), "{:#?}", set_report.unevaluated());
    Ok(())
}

#[test]
fn a_claimed_profile_not_validated_against_is_named_unevaluated() -> TestResult {
    let mut resource = patient();
    resource["meta"] =
        json!({"profile": [patient_url(), "http://example.org/fhir/StructureDefinition/other"]});
    let report = against_patient(&resource)?;
    let claims: Vec<_> = report
        .unevaluated()
        .iter()
        .filter_map(|entry| match &entry.constraint {
            Constraint::ClaimedProfile { url } => Some(url.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        claims,
        ["http://example.org/fhir/StructureDefinition/other"]
    );
    Ok(())
}

#[test]
fn every_issue_and_unevaluated_constraint_converts_into_the_outcome() -> TestResult {
    let mut resource = patient();
    resource["gender"] = json!("male");
    let report = against_patient(&resource)?;
    let outcome = OperationOutcome::from(&report);
    assert_eq!(
        outcome.issue.len(),
        report.issues().len() + report.unevaluated().len()
    );
    let fixed = outcome
        .issue
        .iter()
        .find(|issue| {
            issue
                .details
                .as_ref()
                .and_then(|details| details.coding.first())
                .and_then(|coding| coding.code.as_ref())
                .and_then(|code| code.value.as_deref())
                == Some("fixed-mismatch")
        })
        .expect("the fixed mismatch is an issue");
    assert_eq!(fixed.severity.value.as_deref(), Some("error"));
    assert_eq!(fixed.code.value.as_deref(), Some("value"));
    assert_eq!(fixed.expression[0].value.as_deref(), Some("Patient.gender"));
    let details = fixed.details.as_ref().expect("details");
    assert_eq!(
        details.coding[0]
            .system
            .as_ref()
            .and_then(|s| s.value.as_deref()),
        Some(MESSAGE_SYSTEM)
    );
    assert!(
        details
            .text
            .as_ref()
            .and_then(|t| t.value.as_deref())
            .is_some_and(|text| text.contains("\"female\""))
    );
    let unevaluated = outcome
        .issue
        .iter()
        .filter(|issue| issue.code.value.as_deref() == Some("not-supported"))
        .count();
    assert_eq!(unevaluated, report.unevaluated().len());
    assert!(unevaluated > 0, "the core invariants are named");
    Ok(())
}

#[test]
fn an_empty_report_converts_into_one_information_issue() {
    let outcome = OperationOutcome::from(&Report::default());
    assert_eq!(outcome.issue.len(), 1);
    assert_eq!(
        outcome.issue[0].severity.value.as_deref(),
        Some("information")
    );
    assert_eq!(
        outcome.issue[0].code.value.as_deref(),
        Some("informational")
    );
}

#[test]
fn the_outcome_encodes_as_r4_json() -> TestResult {
    use fhir_types::codec::Json as _;
    let report = against_patient(&patient())?;
    let outcome = OperationOutcome::from(&report);
    let object = outcome.to_json()?;
    assert!(
        object.contains_key("issue"),
        "an OperationOutcome carries issue"
    );
    Ok(())
}

#[test]
fn the_target_profiles_of_a_reference_reached_are_named_unevaluated() -> TestResult {
    let set = PackageSet::load([core()])?;
    let mut resource = patient();
    resource["generalPractitioner"] = json!([{"reference": "Practitioner/p1"}]);
    let report = validate(&set, &resource, None)?;
    let target = report
        .unevaluated()
        .iter()
        .find(|entry| {
            entry.element == "Patient.generalPractitioner"
                && matches!(entry.constraint, Constraint::TargetProfile { .. })
        })
        .expect("the target profiles are listed");
    assert!(
        matches!(&target.constraint, Constraint::TargetProfile { profiles } if profiles.len() == 3),
        "{target:#?}"
    );
    assert_eq!(target.reason, Reason::Profiles);
    Ok(())
}

#[test]
fn the_context_of_a_resolved_extension_is_named_unevaluated() -> TestResult {
    let mut resource = patient();
    resource["extension"] = json!([{"url": format!("{BASE}flag"), "valueBoolean": true}]);
    let report = against_patient(&resource)?;
    let context = report
        .unevaluated()
        .iter()
        .find(|entry| entry.constraint == Constraint::ExtensionContext)
        .expect("the extension context is listed");
    assert_eq!(context.structure, format!("{BASE}flag"));
    assert_eq!(context.location, "Patient.extension[0]");
    Ok(())
}
