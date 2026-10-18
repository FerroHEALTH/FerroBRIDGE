// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The precision functions FHIRPath 3.0.0 adds as Standard for Trial Use:
//! `lowBoundary()`, `highBoundary()` and `precision()`
//! (<http://hl7.org/fhirpath/STU3/#utility-functions>), and `comparable()`
//! (<http://hl7.org/fhirpath/STU3/#comparableother--quantity--boolean>). They
//! are reached only with `Environment::with_fhirpath_stu3_functions`.

use rust_decimal::{Decimal, RoundingStrategy};

use super::Call;
use crate::eval::convert::{describe, integer, single_value};
use crate::eval::error::EvalError;
use crate::temporal::boundary::Boundary;
use crate::value::{Collection, Item};

/// The names this module defines, with their admitted argument counts.
const FUNCTIONS: [(&str, usize, usize, &str); 4] = [
    ("lowBoundary", 0, 1, "0 or 1"),
    ("highBoundary", 0, 1, "0 or 1"),
    ("precision", 0, 0, "no"),
    ("comparable", 1, 1, "1"),
];

/// The precision a decimal boundary takes when none is given: FHIRPath
/// 3.0.0 asks for "at least 8".
const DECIMAL_DIGITS: u32 = 8;

/// The greatest decimal precision this implementation holds, the scale
/// `rust_decimal` carries.
const MAX_DECIMAL_DIGITS: u32 = 28;

/// Calls the function when this module defines it.
pub(super) fn call<'a>(call: &Call<'_, '_, 'a>) -> Result<Option<Collection<'a>>, EvalError> {
    let Some((_, min, max, expected)) = FUNCTIONS.iter().find(|(name, ..)| *name == call.name)
    else {
        return Ok(None);
    };
    call.arity(*min, *max, expected)?;
    let model = call.model();
    let Some(item) = single_value(model, &call.input, call.name)? else {
        return Ok(Some(Vec::new()));
    };
    let result = match call.name {
        "precision" => match precision(&item) {
            Some(digits) => Some(Item::Integer(i64::from(digits))),
            None => return Err(EvalError::type_error(call.name, describe(&item))),
        },
        "comparable" => {
            let Some(other) = single_value(model, &call.arg_or_empty(0)?, call.name)? else {
                return Ok(Some(Vec::new()));
            };
            match (&item, &other) {
                (Item::Quantity(left), Item::Quantity(right)) => {
                    Some(Item::Boolean(left.compare(right).is_some()))
                }
                // NOTE: FHIRPath 3.0.0 comparable(): if either input is not a single Quantity
                // value, the result is empty.
                _ => None,
            }
        }
        _ => {
            let end = if call.name == "lowBoundary" {
                Boundary::Low
            } else {
                Boundary::High
            };
            let digits = integer(model, &call.arg_or_empty(0)?, call.name)?;
            boundary(&item, end, digits, call.name)?
        }
    };
    Ok(Some(result.into_iter().collect()))
}

/// The digits of precision of `item`, `None` for a type `precision()` does
/// not take.
fn precision(item: &Item<'_>) -> Option<u8> {
    match item {
        Item::Decimal(value) => u8::try_from(value.scale()).ok(),
        Item::Integer(_) => Some(0),
        Item::Date(date) => Some(date.digits()),
        Item::DateTime(value) => Some(value.digits()),
        Item::Time(time) => Some(time.digits()),
        _ => None,
    }
}

/// The boundary of `item` at `end` to `digits` of precision, or to the
/// greatest precision of its type when `digits` is `None`.
fn boundary<'a>(
    item: &Item<'a>,
    end: Boundary,
    digits: Option<i64>,
    function: &str,
) -> Result<Option<Item<'a>>, EvalError> {
    // NOTE: FHIRPath 3.0.0 is silent on a negative precision; no specification governs
    // this: our own design answers empty, as for a precision above the greatest held.
    let Ok(digits) = digits.map(u32::try_from).transpose() else {
        return Ok(None);
    };
    Ok(match item {
        Item::Decimal(value) => decimal(*value, end, digits).map(Item::Decimal),
        Item::Integer(value) => decimal(Decimal::from(*value), end, digits).map(Item::Decimal),
        Item::Date(date) => {
            // NOTE: FHIRPath 3.0.0 lowBoundary() names "4 for Date" as the default beside "the
            // greatest precision of the type", which for a date is 8; the greatest is taken.
            temporal(digits, 8).and_then(|digits| date.boundary(end, digits).map(Item::Date))
        }
        Item::DateTime(value) => {
            temporal(digits, 17).and_then(|digits| value.boundary(end, digits).map(Item::DateTime))
        }
        Item::Time(time) => {
            temporal(digits, 9).and_then(|digits| time.boundary(end, digits).map(Item::Time))
        }
        other => return Err(EvalError::type_error(function, describe(other))),
    })
}

/// The precision of a date or time boundary: the one asked for, or the
/// greatest of the type, `None` when it does not fit.
fn temporal(digits: Option<u32>, greatest: u8) -> Option<u8> {
    match digits {
        Some(digits) => u8::try_from(digits).ok(),
        None => Some(greatest),
    }
}

/// The least or greatest decimal `value` stands for, to `digits` places.
///
/// A decimal written to `s` places stands for the values within half a unit
/// of its last place, so the low end is `value - 0.5 * 10^-s` rounded down
/// and the high end `value + 0.5 * 10^-s` rounded up, to `digits` places:
/// `1.587` is `1.5865` to `1.5875`, `1.58` and `1.59` to two places.
fn decimal(value: Decimal, end: Boundary, digits: Option<u32>) -> Option<Decimal> {
    let digits = digits.unwrap_or(DECIMAL_DIGITS);
    let scale = value.scale().checked_add(1)?;
    if digits > MAX_DECIMAL_DIGITS || scale > MAX_DECIMAL_DIGITS {
        return None;
    }
    let half = Decimal::new(5, scale);
    let (edge, strategy) = match end {
        Boundary::Low => (
            value.checked_sub(half)?,
            RoundingStrategy::ToNegativeInfinity,
        ),
        Boundary::High => (
            value.checked_add(half)?,
            RoundingStrategy::ToPositiveInfinity,
        ),
    };
    let mut rounded = edge.round_dp_with_strategy(digits, strategy);
    rounded.rescale(digits);
    Some(rounded)
}
