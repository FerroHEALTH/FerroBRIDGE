// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Equality (`=`, N1 §6.1.1) and equivalence (`~`, N1 §6.1.2 with the FHIR
//! R4 changes of §2.9.1.6) of items and collections.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use rust_decimal::Decimal;

use super::convert::system;
use super::error::EvalError;
use crate::model::Model;
use crate::quantity::{Quantity, Unit, equivalent_decimals};
use crate::value::node::Node;
use crate::value::{Collection, Item};

/// Whether two items are equal, `None` when unknown.
///
/// # Errors
///
/// Returns [`EvalError::Document`] when a node contradicts the model.
pub fn equal(
    model: &dyn Model,
    left: &Item<'_>,
    right: &Item<'_>,
) -> Result<Option<bool>, EvalError> {
    let (Some(left), Some(right)) = (system(model, left)?, system(model, right)?) else {
        return Ok(None);
    };
    Ok(match (&left, &right) {
        (Item::Boolean(a), Item::Boolean(b)) => Some(a == b),
        (Item::String(a), Item::String(b)) => Some(a == b),
        (Item::Integer(a), Item::Integer(b)) => Some(a == b),
        (Item::Date(a), Item::Date(b)) => a.compare(*b).map(|order| order == Ordering::Equal),
        (Item::Time(a), Item::Time(b)) => a.compare(b).map(|order| order == Ordering::Equal),
        (Item::Quantity(a), Item::Quantity(b)) => a.equals(b),
        (Item::Node(a), Item::Node(b)) => nodes_equal(model, a, b)?,
        (Item::Type(a), Item::Type(b)) => Some(a == b),
        _ => {
            if let (Some(a), Some(b)) = (number(&left), number(&right)) {
                Some(a == b)
            } else if let (Some(a), Some(b)) = (date_time(&left), date_time(&right)) {
                a.compare(&b).map(|order| order == Ordering::Equal)
            } else if let (Some(a), Some(b)) = (quantity(&left), quantity(&right)) {
                a.equals(&b)
            } else {
                Some(false)
            }
        }
    })
}

/// Whether two items are equivalent.
///
/// # Errors
///
/// Returns [`EvalError::Document`] when a node contradicts the model.
pub fn equivalent(model: &dyn Model, left: &Item<'_>, right: &Item<'_>) -> Result<bool, EvalError> {
    let (left, right) = match (system(model, left)?, system(model, right)?) {
        (Some(left), Some(right)) => (left, right),
        (None, None) => return Ok(true),
        _ => return Ok(false),
    };
    Ok(match (&left, &right) {
        (Item::Boolean(a), Item::Boolean(b)) => a == b,
        (Item::String(a), Item::String(b)) => fold(a) == fold(b),
        (Item::Integer(a), Item::Integer(b)) => a == b,
        (Item::Date(a), Item::Date(b)) => a.equivalent(*b),
        (Item::Time(a), Item::Time(b)) => a.equivalent(b),
        (Item::Quantity(a), Item::Quantity(b)) => a.equivalent(b) == Some(true),
        (Item::Node(a), Item::Node(b)) => nodes_equivalent(model, a, b)?,
        (Item::Type(a), Item::Type(b)) => a == b,
        _ => {
            if let (Some(a), Some(b)) = (number(&left), number(&right)) {
                equivalent_decimals(a, b)
            } else if let (Some(a), Some(b)) = (date_time(&left), date_time(&right)) {
                a.equivalent(&b)
            } else if let (Some(a), Some(b)) = (quantity(&left), quantity(&right)) {
                a.equivalent(&b) == Some(true)
            } else {
                false
            }
        }
    })
}

/// A string as string equivalence compares it: case ignored, every
/// whitespace character one (N1 §6.1.2).
fn fold(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .flat_map(char::to_lowercase)
        .collect()
}

/// An Integer or Decimal as a decimal, the implicit conversion of N1 §5.5.
#[must_use]
pub fn number(item: &Item<'_>) -> Option<Decimal> {
    match item {
        Item::Integer(value) => Some(Decimal::from(*value)),
        Item::Decimal(value) => Some(*value),
        _ => None,
    }
}

/// A `Date` or `DateTime` as a `DateTime`, the implicit conversion of N1 §5.5.
#[must_use]
pub fn date_time(item: &Item<'_>) -> Option<crate::temporal::DateTime> {
    match item {
        Item::Date(value) => Some(value.to_date_time()),
        Item::DateTime(value) => Some(*value),
        _ => None,
    }
}

/// A number or a quantity as a quantity, a number taking the unit `'1'` (N1
/// §5.5).
#[must_use]
pub fn quantity(item: &Item<'_>) -> Option<Quantity> {
    match item {
        Item::Quantity(value) => Some(value.clone()),
        other => number(other).map(|value| Quantity::new(value, Unit::one())),
    }
}

/// A node's children grouped by member name.
fn grouped<'a>(
    model: &dyn Model,
    node: &Node<'a>,
    skip_id: bool,
) -> Result<BTreeMap<String, Collection<'a>>, EvalError> {
    let mut groups: BTreeMap<String, Collection<'a>> = BTreeMap::new();
    for (name, item) in node.named_children(model)? {
        if skip_id && name == "id" {
            continue;
        }
        groups.entry(name).or_default().push(item);
    }
    Ok(groups)
}

/// The member `name` of a node, empty when its type does not define it.
fn members<'n>(
    model: &dyn Model,
    node: &Node<'n>,
    name: &str,
) -> Result<Collection<'n>, EvalError> {
    Ok(node.member(model, name)?.unwrap_or_default())
}

/// Equality of two complex nodes: every child property equal, recursively
/// (N1 §6.1.1).
fn nodes_equal(
    model: &dyn Model,
    left: &Node<'_>,
    right: &Node<'_>,
) -> Result<Option<bool>, EvalError> {
    if let (Some(a), Some(b)) = (left.fhir_type(), right.fhir_type())
        && a.name != b.name
    {
        return Ok(Some(false));
    }
    let (left, right) = (grouped(model, left, false)?, grouped(model, right, false)?);
    if !left.keys().eq(right.keys()) {
        return Ok(Some(false));
    }
    let mut unknown = false;
    for (name, items) in &left {
        let others = right.get(name).map_or(&[][..], Vec::as_slice);
        match collections_equal(model, items, others)? {
            Some(false) => return Ok(Some(false)),
            None => unknown = true,
            Some(true) => {}
        }
    }
    Ok(if unknown { None } else { Some(true) })
}

/// Equivalence of two complex nodes, FHIR R4 §2.9.1.6: ids ignored, a
/// `Coding` by its system and code, a `CodeableConcept` by any shared
/// coding.
fn nodes_equivalent(
    model: &dyn Model,
    left: &Node<'_>,
    right: &Node<'_>,
) -> Result<bool, EvalError> {
    let (a, b) = (
        left.fhir_type().map(|ty| ty.name),
        right.fhir_type().map(|ty| ty.name),
    );
    if let (Some(a), Some(b)) = (a, b)
        && a != b
    {
        return Ok(false);
    }
    match a.or(b) {
        Some("Coding") => {
            for name in ["system", "code"] {
                let (mine, theirs) = (members(model, left, name)?, members(model, right, name)?);
                if !collections_equivalent(model, &mine, &theirs)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Some("CodeableConcept") => {
            let (codings, others) = (
                members(model, left, "coding")?,
                members(model, right, "coding")?,
            );
            for coding in &codings {
                for other in &others {
                    if equivalent(model, coding, other)? {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
        _ => {
            let (left, right) = (grouped(model, left, true)?, grouped(model, right, true)?);
            if !left.keys().eq(right.keys()) {
                return Ok(false);
            }
            for (name, items) in &left {
                let others = right.get(name).map_or(&[][..], Vec::as_slice);
                if !collections_equivalent(model, items, others)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

/// `=` over collections (N1 §6.1.1): empty when either is empty, `false` for
/// different lengths, else every item equal in order.
///
/// # Errors
///
/// Returns [`EvalError::Document`] when a node contradicts the model.
pub fn collections_equal(
    model: &dyn Model,
    left: &[Item<'_>],
    right: &[Item<'_>],
) -> Result<Option<bool>, EvalError> {
    if left.is_empty() || right.is_empty() {
        return Ok(None);
    }
    if left.len() != right.len() {
        return Ok(Some(false));
    }
    let mut unknown = false;
    for (a, b) in left.iter().zip(right) {
        match equal(model, a, b)? {
            Some(false) => return Ok(Some(false)),
            None => unknown = true,
            Some(true) => {}
        }
    }
    Ok(if unknown { None } else { Some(true) })
}

/// `~` over collections (N1 §6.1.2): two empty collections are equivalent,
/// otherwise every item has an equivalent counterpart, in any order.
///
/// # Errors
///
/// Returns [`EvalError::Document`] when a node contradicts the model.
pub fn collections_equivalent(
    model: &dyn Model,
    left: &[Item<'_>],
    right: &[Item<'_>],
) -> Result<bool, EvalError> {
    if left.len() != right.len() {
        return Ok(false);
    }
    let mut used = vec![false; right.len()];
    for item in left {
        let mut matched = false;
        for (index, other) in right.iter().enumerate() {
            if used.get(index).copied().unwrap_or(true) {
                continue;
            }
            if equivalent(model, item, other)? {
                if let Some(slot) = used.get_mut(index) {
                    *slot = true;
                }
                matched = true;
                break;
            }
        }
        if !matched {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether `collection` holds an item equal to `item`.
///
/// # Errors
///
/// Returns [`EvalError::Document`] when a node contradicts the model.
pub fn contains(
    model: &dyn Model,
    collection: &[Item<'_>],
    item: &Item<'_>,
) -> Result<bool, EvalError> {
    for candidate in collection {
        if equal(model, candidate, item)? == Some(true) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The distinct items of `items`, the first of each kept in order.
///
/// # Errors
///
/// Returns [`EvalError::Document`] when a node contradicts the model.
pub fn distinct<'a>(model: &dyn Model, items: Collection<'a>) -> Result<Collection<'a>, EvalError> {
    let mut kept: Collection<'a> = Vec::with_capacity(items.len());
    for item in items {
        if !contains(model, &kept, &item)? {
            kept.push(item);
        }
    }
    Ok(kept)
}
