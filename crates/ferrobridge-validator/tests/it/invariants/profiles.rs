// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The invariants a profile's snapshot declares: on elements below the root,
//! with their own severity, evaluated once per occurrence when the snapshot
//! repeats a core constraint, and listed when the expression cannot be used.

use ferrobridge_validator::issue::Severity;
use ferrobridge_validator::report::InvariantStage;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use serde_json::json;

use super::failed_at;
use super::failures;
use super::listed;
use crate::support::BASE;
use crate::support::TestResult;
use crate::support::profile;
use crate::support::set_with;
use crate::support::validate;

/// The canonical of the profile under test.
fn url() -> String {
    format!("{BASE}patient-invariants")
}

/// One constraint of the profile.
fn constraint(key: &str, severity: &str, expression: Option<&str>) -> serde_json::Value {
    let mut constraint = json!({
        "key": key, "severity": severity, "human": format!("the synthetic rule {key}"), "source": url()
    });
    if let Some(expression) = expression {
        constraint["expression"] = json!(expression);
    }
    constraint
}

/// A Patient profile whose root carries a warning, an expression that does
/// not parse, one that names no element of Patient and one without an
/// expression, and whose names carry `ele-1` as the core declares it and a
/// rule of their own.
fn invariants_profile() -> serde_json::Value {
    profile(
        "patient-invariants",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*", "constraint": [
                constraint("test-warn", "warning", Some("birthDate.exists()")),
                constraint("test-parse", "error", Some("name.exists(")),
                constraint("test-check", "error", Some("nonsense.exists()")),
                constraint("test-none", "error", None)
            ]},
            {"id": "Patient.name", "path": "Patient.name", "min": 0, "max": "*",
             "type": [{"code": "HumanName"}], "constraint": [
                {"key": "ele-1", "severity": "error",
                 "human": "All FHIR elements must have a @value or children",
                 "expression": "hasValue() or (children().count() > id.count())",
                 "source": "http://hl7.org/fhir/StructureDefinition/Element"},
                constraint("test-family", "error", Some("family.exists()"))
            ]}
        ]),
    )
}

/// Validates the Patient with `names` against the profile.
fn against_profile(names: &serde_json::Value) -> TestResult<Report> {
    let (_synthetic, set) = set_with(&[invariants_profile()])?;
    validate(
        &set,
        &json!({"resourceType": "Patient", "name": names}),
        Some(&url()),
    )
}

#[test]
fn a_profile_rule_below_the_root_is_evaluated_on_each_occurrence() -> TestResult {
    let report = against_profile(&json!([{"family": "Doe"}, {"given": ["Ada"]}]))?;
    assert_eq!(failed_at(&report, "test-family"), ["Patient.name[1]"]);
    Ok(())
}

#[test]
fn a_core_rule_the_profile_repeats_is_evaluated_once_per_occurrence() -> TestResult {
    let report = against_profile(&json!([{"id": "n1"}]))?;
    assert_eq!(failed_at(&report, "ele-1"), ["Patient.name[0]"]);
    Ok(())
}

#[test]
fn a_warning_rule_is_reported_as_a_warning() -> TestResult {
    let report = against_profile(&json!([{"family": "Doe"}]))?;
    let found = failures(&report, "test-warn");
    assert_eq!(found.len(), 1, "{:#?}", report.issues());
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(found[0].expression, "Patient");
    Ok(())
}

#[test]
fn an_expression_that_does_not_parse_or_fit_is_listed_with_its_error() -> TestResult {
    let report = against_profile(&json!([{"family": "Doe"}]))?;
    for (key, stage) in [
        ("test-parse", InvariantStage::Parse),
        ("test-check", InvariantStage::Check),
    ] {
        assert!(failed_at(&report, key).is_empty(), "{key} is never failed");
        let entries = listed(&report, key);
        assert_eq!(entries.len(), 1, "{key}: {:#?}", report.unevaluated());
        assert_eq!(entries[0].structure, url());
        assert_eq!(entries[0].element, "Patient");
        assert!(
            matches!(&entries[0].reason, Reason::InvariantRefused { stage: found, upstream: None, .. } if *found == stage),
            "{key}: {:#?}",
            entries[0].reason
        );
    }
    let none = listed(&report, "test-none");
    assert_eq!(none.len(), 1, "{:#?}", report.unevaluated());
    assert_eq!(none[0].reason, Reason::NoExpression);
    Ok(())
}
