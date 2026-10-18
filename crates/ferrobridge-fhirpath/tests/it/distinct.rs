// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `distinct()` and `isDistinct()` keep N1 §5.1.4 equality (`=`, §6.1.1) on
//! every kind of item and stay near-linear on a large collection.

use std::time::{Duration, Instant};

use ferrobridge_fhirpath::eval::equality::{distinct, equal};
use ferrobridge_fhirpath::model::r4::R4;
use ferrobridge_fhirpath::quantity::Quantity;
use ferrobridge_fhirpath::value::{Collection, Item};
use rust_decimal::Decimal;

use super::support::{eval, eval_none, json};

/// The distinct items by the pairwise definition: an item is kept when no
/// kept item is equal to it.
fn reference<'a>(items: &Collection<'a>) -> Collection<'a> {
    let mut kept: Collection<'a> = Vec::new();
    for item in items {
        let mut seen = false;
        for candidate in &kept {
            if equal(&R4, candidate, item).expect("compares") == Some(true) {
                seen = true;
                break;
            }
        }
        if !seen {
            kept.push(item.clone());
        }
    }
    kept
}

fn quantity(text: &str) -> Item<'static> {
    Item::Quantity(Quantity::parse(text).expect("a quantity"))
}

#[test]
fn distinct_agrees_with_pairwise_equality_on_every_kind() {
    // N1 §5.1.4: distinct() keeps the items no earlier item is equal (=) to.
    let mut items: Collection<'static> = eval_none(
        "(1 | 1.0 | 2 | 'a' | 'A' | true | false | @2014 | @2014-01-01 | @T10:30 | @T10:30:00)",
    )
    .expect("evaluates");
    items.extend([
        Item::Integer(1),
        Item::Decimal(Decimal::new(100, 2)),
        Item::String(String::from("a")),
        Item::Boolean(true),
        quantity("1 'm'"),
        quantity("100 'cm'"),
        quantity("1 '1'"),
        quantity("3 'days'"),
        quantity("3 'd'"),
    ]);
    let found = distinct(&R4, items.clone()).expect("distinct");
    let expected = reference(&items);
    assert_eq!(found, expected);
}

#[test]
fn a_number_and_a_dimensionless_quantity_are_one_item() {
    // N1 §5.5: a number converts to a quantity of unit '1', and = compares them.
    let items = vec![Item::Integer(1), quantity("1 '1'"), quantity("100 '%'")];
    assert_eq!(
        distinct(&R4, items.clone()).expect("distinct"),
        reference(&items)
    );
}

#[test]
fn distinct_elements_compare_by_their_children() {
    // N1 §6.1.1: complex types are equal when all child properties are equal.
    let patient = json(
        r#"{"resourceType": "Patient", "name": [
            {"family": "Doe", "given": ["Jane"]},
            {"family": "Doe", "given": ["Jane"]},
            {"family": "Roe"}
        ]}"#,
    );
    let result = eval("Patient.name.distinct().count()", &patient).expect("evaluates");
    assert_eq!(result, vec![Item::Integer(2)]);
    let result = eval("Patient.name.isDistinct()", &patient).expect("evaluates");
    assert_eq!(result, vec![Item::Boolean(false)]);
}

#[test]
fn distinct_over_twenty_thousand_strings_is_near_linear() {
    // Each item is compared within its value's bucket only, so 20000 distinct strings,
    // 400 million comparisons pairwise, take a map lookup each.
    let items: Collection<'static> = (0..20_000)
        .map(|index| Item::String(format!("urn:uuid:{index:08}")))
        .chain((0..20_000).map(|index| Item::String(format!("urn:uuid:{index:08}"))))
        .collect();
    let started = Instant::now();
    let found = distinct(&R4, items).expect("distinct");
    let elapsed = started.elapsed();
    assert_eq!(found.len(), 20_000);
    assert!(
        elapsed < Duration::from_secs(10),
        "distinct over 40000 strings took {elapsed:?}"
    );
}
