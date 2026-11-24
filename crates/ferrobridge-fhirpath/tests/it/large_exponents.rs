// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FHIR R4 `decimal` the exact decimal cannot hold with its stated
//! precision (datatypes.html §2.24.0.1 admits `1e999999999`): read without
//! writing it out, compared exactly, and empty from every computation.

use ferrobridge_fhirpath::eval::error::EvalError;
use ferrobridge_fhirpath::value::Item;

use super::support::{eval, eval_none, json, truth};

/// An Observation whose quantity has the value `value` in `code`.
fn observation(value: &str, code: &str) -> fhir_types::codec::Value {
    json(&format!(
        r#"{{"resourceType": "Observation", "status": "final", "code": {{"text": "t"}},
            "valueQuantity": {{"value": {value}, "system": "http://unitsofmeasure.org", "code": "{code}"}},
            "component": [{{"code": {{"text": "c"}}, "valueQuantity": {{"value": {value}}}}}]}}"#
    ))
}

fn holds(expression: &str, resource: &fhir_types::codec::Value) -> Option<bool> {
    truth(&eval(expression, resource).expect("evaluates"))
}

fn empty(expression: &str, resource: &fhir_types::codec::Value) -> bool {
    eval(expression, resource).expect("evaluates").is_empty()
}

#[test]
fn a_huge_value_reads_as_a_decimal_and_a_quantity() {
    // FHIR R4 fhirpath.html §2.9.1.2 and §2.9.1.3: a decimal is a System.Decimal and a UCUM
    // Quantity a System.Quantity.
    let huge = observation("1e999999999", "kg");
    assert_eq!(holds("value is Quantity", &huge), Some(true));
    assert_eq!(
        holds("component.value.value.toDecimal() is System.Decimal", &huge),
        Some(true)
    );
}

#[test]
fn a_huge_value_compares_exactly() {
    // N1 §6.1.1, §6.1.2 and §6.2.
    let huge = observation("1e999999999", "kg");
    assert_eq!(holds("value = 1 'kg'", &huge), Some(false));
    assert_eq!(holds("value ~ 1 'kg'", &huge), Some(false));
    assert_eq!(holds("value > 1 'kg'", &huge), Some(true));
    assert_eq!(holds("component.value.value > 1", &huge), Some(true));
    assert_eq!(holds("-component.value.value < 1", &huge), Some(true));
    assert_eq!(holds("value = value", &huge), Some(true));
}

#[test]
fn a_huge_value_in_another_unit_does_not_compare() {
    // N1 §6.6: the conversion overflows, so equality and order are empty, and equivalence
    // false as for units that do not convert (§6.1.2).
    let huge = observation("1e999999999", "kg");
    assert!(empty("value = 1 'g'", &huge));
    assert!(empty("value < 1 'g'", &huge));
    assert_eq!(holds("value ~ 1 'g'", &huge), Some(false));
}

#[test]
fn a_tiny_value_is_equivalent_to_zero_at_its_precision() {
    // N1 §6.1.2: decimals compare rounded to the precision of the less precise operand.
    let tiny = observation("1e-999999999", "kg");
    assert_eq!(holds("value ~ 0 'kg'", &tiny), Some(true));
    assert_eq!(holds("value = 0 'kg'", &tiny), Some(false));
    assert_eq!(holds("component.value.value > 0", &tiny), Some(true));
}

#[test]
fn every_computation_on_a_huge_value_is_empty() {
    // N1 §6.6: an operation that overflows or underflows is empty, and §5.7 for the math
    // functions.
    let huge = observation("1e999999999", "kg");
    for expression in [
        "component.value.value + 1",
        "component.value.value - 1",
        "component.value.value * 2",
        "component.value.value / 2",
        "component.value.value div 2",
        "component.value.value mod 2",
        "component.value.value.round(2)",
        "component.value.value.abs()",
        "component.value.value.sqrt()",
        "value * 2 'kg'",
        "value + 1 'kg'",
    ] {
        assert!(empty(expression, &huge), "{expression}");
    }
}

#[test]
fn a_huge_duration_moves_a_date_past_its_years() {
    // N1 §6.7: a resulting year out of range is an error.
    let huge = observation("1e999999999", "d");
    assert!(matches!(
        eval("@2020-01-01 + value", &huge),
        Err(EvalError::Arithmetic { .. })
    ));
}

#[test]
fn a_huge_value_is_too_long_for_to_string() {
    // N1 §5.5.8 writes a Decimal in fixed-point form only.
    let huge = observation("1e999999999", "kg");
    assert!(matches!(
        eval("value.toString()", &huge),
        Err(EvalError::Unwritable {
            digits: 1_000_000_000
        })
    ));
    assert_eq!(holds("value.convertsToString()", &huge), Some(false));
}

#[test]
fn a_long_value_is_written_out_in_fixed_point() {
    // N1 §5.5.8: `(\+|-)?\d+(.\d+)?`, the stated digits kept.
    let long = format!("1.{}1", "0".repeat(30));
    let resource = observation(&long, "kg");
    let written = eval("value.toString()", &resource).expect("evaluates");
    assert_eq!(written, [Item::String(format!("{long} 'kg'"))]);
    assert_eq!(holds("value = 1 'kg'", &resource), Some(false));
    assert_eq!(holds("value ~ 1 'kg'", &resource), Some(true));
}

#[test]
fn a_value_with_trailing_zeros_past_the_exact_scale_equals_its_number() {
    // N1 §6.1.1: trailing zeroes after the decimal are ignored.
    let zeros = format!("1.{}", "0".repeat(30));
    let resource = observation(&zeros, "kg");
    assert_eq!(holds("component.value.value = 1", &resource), Some(true));
    assert_eq!(
        eval("component.value.value | 1", &resource)
            .expect("evaluates")
            .len(),
        1
    );
}

#[test]
fn a_rounding_precision_past_the_exact_scale_is_empty() {
    // N1 §6.6: the result overflows the places the exact decimal holds.
    for expression in ["1.5.round(2000000000)", "1.5.round(29)"] {
        assert_eq!(
            eval_none(expression).expect("evaluates").len(),
            0,
            "{expression}"
        );
    }
    assert_eq!(eval_none("1.5.round(28)").expect("evaluates").len(), 1);
}

#[test]
fn a_negative_rounding_precision_is_an_error() {
    // N1 §5.7.8: the precision must be >= 0 or the evaluation ends with an error.
    assert!(matches!(
        eval_none("1.5.round(-1)"),
        Err(EvalError::Argument { .. })
    ));
}
