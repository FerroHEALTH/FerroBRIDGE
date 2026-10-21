// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An invariant on a choice element a profile narrows. "Constraints limiting
//! the acceptable list of types must be applied to the original "[x]"
//! element" (<https://hl7.org/fhir/R4/elementdefinition.html>, §2.30.0.4.2),
//! so an invariant the profile itself declares there (its
//! `constraint.source`) is checked against the types the profile keeps, and
//! one it inherits against every type the base choice admits.

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

/// The canonical of the profile under test.
fn url() -> String {
    format!("{BASE}observation-quantity")
}

/// An Observation profile narrowing `value[x]` to Quantity, carrying the
/// constraint `key` with `expression`, declared by `source`.
fn narrowed(key: &str, expression: &str, source: &str) -> serde_json::Value {
    profile(
        "observation-quantity",
        "Observation",
        &json!([
            {"id": "Observation", "path": "Observation", "min": 0, "max": "*"},
            {"id": "Observation.value[x]", "path": "Observation.value[x]", "min": 0, "max": "1",
             "type": [{"code": "Quantity"}],
             "constraint": [{"key": key, "severity": "error", "human": format!("the synthetic rule {key}"),
                             "expression": expression, "source": source}]}
        ]),
    )
}

/// Validates an Observation whose value is `quantity` against the profile.
fn observed(definition: serde_json::Value, quantity: &serde_json::Value) -> TestResult<Report> {
    let (_synthetic, set) = set_with(&[definition])?;
    let resource = json!({"resourceType": "Observation", "status": "final", "code": {"text": "x"},
                          "valueQuantity": quantity});
    validate(&set, &resource, Some(&url()))
}

#[test]
fn a_profile_invariant_naming_a_member_only_a_removed_type_defines_is_refused() -> TestResult {
    // `coding` belongs to CodeableConcept, which the base choice admits and the profile removes.
    let report = observed(
        narrowed("test-coding", "coding.exists()", &url()),
        &json!({"value": 1}),
    )?;
    assert_eq!(failed_at(&report, "test-coding"), Vec::<String>::new());
    let entries = listed(&report, "test-coding");
    assert_eq!(entries.len(), 1, "{:#?}", report.unevaluated());
    assert!(
        matches!(
            &entries[0].reason,
            Reason::InvariantRefused {
                stage: InvariantStage::Check,
                ..
            }
        ),
        "{:#?}",
        entries[0].reason
    );
    Ok(())
}

#[test]
fn a_profile_invariant_on_a_kept_type_is_evaluated() -> TestResult {
    let definition = || narrowed("test-unit", "unit.exists()", &url());
    let report = observed(definition(), &json!({"value": 1, "unit": "mg"}))?;
    assert!(
        failed_at(&report, "test-unit").is_empty(),
        "{:#?}",
        report.issues()
    );
    assert!(
        listed(&report, "test-unit").is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    let report = observed(definition(), &json!({"value": 1}))?;
    assert_eq!(
        failed_at(&report, "test-unit"),
        ["Observation.value.ofType(Quantity)"]
    );
    Ok(())
}

#[test]
fn an_inherited_invariant_is_checked_against_the_base_choice() -> TestResult {
    // The rule comes from another definition, written against the base choice, where
    // CodeableConcept defines `coding`.
    let inherited = format!("{BASE}observation-base");
    let report = observed(
        narrowed("test-inherited", "coding.exists().not()", &inherited),
        &json!({"value": 1}),
    )?;
    assert!(
        listed(&report, "test-inherited").is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    assert!(
        failed_at(&report, "test-inherited").is_empty(),
        "{:#?}",
        report.issues()
    );
    Ok(())
}
