// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The math functions (N1 §5.7, STU).

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, MathematicalOps, RoundingStrategy};

use super::Call;
use crate::eval::convert::{describe, single_value};
use crate::eval::equality::number;
use crate::eval::error::EvalError;
use crate::value::{Collection, Item};

/// The decimal places a result of `exp()`, `ln()`, `log()`, `sqrt()` or a
/// fractional `power()` keeps.
///
/// NOTE: N1 §5.7 fixes no precision for these results; no specification governs this:
/// our own design, the 10^-8 step N1 §4.1.4 sets as the least a decimal must carry.
const TRANSCENDENTAL_PLACES: u32 = 8;

/// A transcendental result rounded to [`TRANSCENDENTAL_PLACES`].
fn settled(value: Decimal) -> Decimal {
    value
        .round_dp_with_strategy(
            TRANSCENDENTAL_PLACES,
            RoundingStrategy::MidpointAwayFromZero,
        )
        .normalize()
}

/// The integer a decimal truncates to, `None` past the 32-bit range.
fn whole(value: Decimal) -> Option<Item<'static>> {
    value
        .to_i64()
        .filter(|value| i32::try_from(*value).is_ok())
        .map(Item::Integer)
}

/// Calls the function when this module defines it.
pub(super) fn call<'a>(call: &Call<'_, '_, 'a>) -> Result<Option<Collection<'a>>, EvalError> {
    let (min, max, expected) = match call.name {
        "abs" | "ceiling" | "exp" | "floor" | "ln" | "sqrt" | "truncate" => (0, 0, "no"),
        "log" | "power" => (1, 1, "1"),
        "round" => (0, 1, "0 or 1"),
        _ => return Ok(None),
    };
    call.arity(min, max, expected)?;
    let model = call.model();
    let Some(item) = single_value(model, &call.input, call.name)? else {
        return Ok(Some(Vec::new()));
    };
    let mismatch = || EvalError::type_error(call.name, describe(&item));
    let result = match (call.name, &item) {
        ("abs", Item::Integer(value)) => value.checked_abs().map(Item::Integer),
        ("abs", Item::Decimal(value)) => Some(Item::Decimal(value.abs())),
        ("abs", Item::Quantity(value)) => {
            let mut absolute = value.clone();
            absolute.value = absolute.value.abs();
            Some(Item::Quantity(absolute))
        }
        ("ceiling" | "floor" | "truncate", Item::Integer(value)) => Some(Item::Integer(*value)),
        ("ceiling", Item::Decimal(value)) => whole(value.ceil()),
        ("floor", Item::Decimal(value)) => whole(value.floor()),
        ("truncate", Item::Decimal(value)) => whole(value.trunc()),
        ("exp" | "ln" | "sqrt" | "log" | "round", _) => {
            let value = number(&item).ok_or_else(mismatch)?;
            match call.name {
                "exp" => value.checked_exp().map(settled).map(Item::Decimal),
                "ln" => (value > Decimal::ZERO)
                    .then(|| value.checked_ln())
                    .flatten()
                    .map(settled)
                    .map(Item::Decimal),
                "sqrt" => (value >= Decimal::ZERO)
                    .then(|| value.sqrt())
                    .flatten()
                    .map(settled)
                    .map(Item::Decimal),
                "log" => {
                    let Some(base) = single_value(model, &call.arg_or_empty(0)?, "log")? else {
                        return Ok(Some(Vec::new()));
                    };
                    let base = number(&base)
                        .ok_or_else(|| EvalError::type_error("log", describe(&base)))?;
                    log(value, base).map(Item::Decimal)
                }
                _ => {
                    let places = match single_value(model, &call.arg_or_empty(0)?, "round")? {
                        None => 0,
                        Some(Item::Integer(places)) => {
                            u32::try_from(places)
                                .ok()
                                .ok_or_else(|| EvalError::Argument {
                                    function: String::from("round"),
                                    reason: String::from("the precision is negative"),
                                })?
                        }
                        Some(other) => {
                            return Err(EvalError::type_error("round", describe(&other)));
                        }
                    };
                    Some(Item::Decimal(value.round_dp_with_strategy(
                        places,
                        RoundingStrategy::MidpointAwayFromZero,
                    )))
                }
            }
        }
        ("power", _) => {
            let Some(exponent) = single_value(model, &call.arg_or_empty(0)?, "power")? else {
                return Ok(Some(Vec::new()));
            };
            power(&item, &exponent)?
        }
        _ => return Err(mismatch()),
    };
    Ok(Some(result.into_iter().collect()))
}

/// The logarithm of `value` in `base`, `None` where it is undefined.
fn log(value: Decimal, base: Decimal) -> Option<Decimal> {
    if value <= Decimal::ZERO || base <= Decimal::ZERO || base == Decimal::ONE {
        return None;
    }
    let numerator = value.checked_ln()?;
    let denominator = base.checked_ln()?;
    numerator.checked_div(denominator).map(settled)
}

/// `power()` (N1 §5.7.7): empty when the power cannot be represented.
///
/// # Errors
///
/// Returns [`EvalError::Type`] when an operand is not a number.
fn power(base: &Item<'_>, exponent: &Item<'_>) -> Result<Option<Item<'static>>, EvalError> {
    let mismatch = || {
        EvalError::type_error(
            "power",
            format!("{} and {}", describe(base), describe(exponent)),
        )
    };
    if let (Item::Integer(base), Item::Integer(exponent)) = (base, exponent) {
        return Ok(u32::try_from(*exponent)
            .ok()
            .and_then(|exponent| base.checked_pow(exponent))
            .filter(|value| i32::try_from(*value).is_ok())
            .map(Item::Integer));
    }
    let base = number(base).ok_or_else(mismatch)?;
    Ok(match exponent {
        Item::Integer(exponent) => base.checked_powi(*exponent).map(Item::Decimal),
        Item::Decimal(exponent) if exponent.fract().is_zero() => exponent
            .to_i64()
            .and_then(|exponent| base.checked_powi(exponent))
            .map(Item::Decimal),
        Item::Decimal(exponent) => (base >= Decimal::ZERO)
            .then(|| base.checked_powd(*exponent))
            .flatten()
            .map(settled)
            .map(Item::Decimal),
        _ => return Err(mismatch()),
    })
}
