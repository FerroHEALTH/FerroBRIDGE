// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The strict ordering mode: N1 §3.1 leaves the order of `children()` and
//! `descendants()` undefined and lets an implementation refuse an
//! order-dependent operation on them.

use ferrobridge_fhirpath::eval::error::EvalError;
use ferrobridge_fhirpath::eval::{Environment, Focus};
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::model::r4::R4;
use ferrobridge_fhirpath::value::Collection;
use fhir_types::codec::Value;

use super::support::json;

fn patient() -> Value {
    json(
        r#"{"resourceType": "Patient", "id": "p1", "active": true,
            "name": [{"family": "Doe", "given": ["Jane", "Ann"]}]}"#,
    )
}

fn strict<'a>(expression: &str, resource: &'a Value) -> Result<Collection<'a>, EvalError> {
    let parsed = Expression::parse(expression).expect("the expression should parse");
    parsed.evaluate(
        &Environment::new(&R4).with_strict_ordering(),
        Focus::resource(resource),
    )
}

fn lenient<'a>(expression: &str, resource: &'a Value) -> Result<Collection<'a>, EvalError> {
    let parsed = Expression::parse(expression).expect("the expression should parse");
    parsed.evaluate(&Environment::new(&R4), Focus::resource(resource))
}

fn refused(result: Result<Collection<'_>, EvalError>, operation: &str) -> bool {
    matches!(result, Err(EvalError::Unordered { operation: found }) if found == operation)
}

#[test]
fn every_ordered_function_over_children_is_refused() {
    // N1 §3.1: first(), last(), tail(), skip() and take() should not be used on
    // collections derived from children() or descendants().
    let patient = patient();
    for (expression, operation) in [
        ("Patient.children().first()", "first"),
        ("Patient.children().last()", "last"),
        ("Patient.children().tail()", "tail"),
        ("Patient.children().skip(1)", "skip"),
        ("Patient.descendants().take(1)", "take"),
    ] {
        assert!(
            refused(strict(expression, &patient), operation),
            "{expression}"
        );
    }
}

#[test]
fn the_indexer_over_descendants_is_refused() {
    // N1 §3.1: an item is addressed by its ordinal position, so the indexer depends on order.
    let patient = patient();
    assert!(refused(strict("Patient.descendants()[0]", &patient), "[]"));
}

#[test]
fn a_collection_derived_from_children_stays_unordered() {
    // N1 §3.1: "collections derived from these paths".
    let patient = patient();
    assert!(refused(
        strict(
            "Patient.children().where($this is HumanName).first()",
            &patient
        ),
        "first"
    ));
    assert!(refused(
        strict("(Patient.name | Patient.children()).last()", &patient),
        "last"
    ));
}

#[test]
fn an_ordered_path_and_a_single_value_are_not_refused() {
    // N1 §3.1: a collection is ordered unless the order is undefined.
    let patient = patient();
    let result = strict("Patient.name.given.first()", &patient).expect("an ordered path");
    assert_eq!(result.len(), 1);
    let result = strict("Patient.children().count()", &patient).expect("a count");
    assert_eq!(result.len(), 1);
}

#[test]
fn a_sorted_collection_is_ordered_again() {
    // FHIRPath 3.0.0 sort() answers the items sorted, so their order is defined.
    let patient = patient();
    let parsed = Expression::parse("Patient.name.given.children().sort().first()")
        .expect("the expression should parse");
    let result = parsed.evaluate(
        &Environment::new(&R4)
            .with_strict_ordering()
            .with_fhirpath_stu3_functions(),
        Focus::resource(&patient),
    );
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn the_mode_is_off_by_default() {
    // N1 §3.1: refusing is an implementation's choice; the default answers in document order.
    let patient = patient();
    let result = lenient("Patient.children().skip(1)", &patient).expect("evaluates");
    assert!(!result.is_empty());
}
