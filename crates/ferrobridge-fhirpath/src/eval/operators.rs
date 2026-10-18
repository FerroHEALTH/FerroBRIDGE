// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operators of N1 §6: comparison, arithmetic, Boolean logic, the
//! collection operators and date and time arithmetic.

use std::cmp::Ordering;

use rust_decimal::Decimal;

use super::convert::{boolean, describe, single_value};
use super::equality::{
    collections_equal, collections_equivalent, contains, date_time, distinct, number, quantity,
};
use super::error::EvalError;
use super::{Engine, Frame};
use crate::ast::{BinaryOp, Expr};
use crate::model::Model;
use crate::quantity::{Calendar, Unit};
use crate::value::{Collection, Item};

/// The lower and upper bound of `System.Integer` (N1 §4.1.3).
const INTEGER_RANGE: std::ops::RangeInclusive<i64> = -2_147_483_648..=2_147_483_647;

/// An integer result, `None` past the 32-bit range: overflow is empty (N1
/// §6.6).
fn integer(value: Option<i64>) -> Option<Item<'static>> {
    value
        .filter(|value| INTEGER_RANGE.contains(value))
        .map(Item::Integer)
}

/// Unary minus.
pub(crate) fn negate<'a>(
    model: &dyn Model,
    operand: &Collection<'a>,
) -> Result<Collection<'a>, EvalError> {
    let value = single_value(model, operand, "unary -")?;
    Ok(match value {
        None => Vec::new(),
        Some(Item::Integer(value)) => integer(value.checked_neg()).into_iter().collect(),
        Some(Item::Decimal(value)) => vec![Item::Decimal(-value)],
        Some(Item::Quantity(mut value)) => {
            value.value = -value.value;
            vec![Item::Quantity(value)]
        }
        Some(other) => return Err(EvalError::type_error("unary -", describe(&other))),
    })
}

/// Unary plus.
pub(crate) fn plus<'a>(
    model: &dyn Model,
    operand: &Collection<'a>,
) -> Result<Collection<'a>, EvalError> {
    let value = single_value(model, operand, "unary +")?;
    Ok(match value {
        None => Vec::new(),
        Some(item @ (Item::Integer(_) | Item::Decimal(_) | Item::Quantity(_))) => vec![item],
        Some(other) => return Err(EvalError::type_error("unary +", describe(&other))),
    })
}

/// A Boolean as a one-item collection, an unknown as the empty one.
fn truth<'a>(value: Option<bool>) -> Collection<'a> {
    value.map(Item::Boolean).into_iter().collect()
}

impl<'a> Engine<'_, 'a> {
    /// Evaluates a binary operator.
    pub(crate) fn binary(
        &self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
        frame: &Frame<'a>,
    ) -> Result<Collection<'a>, EvalError> {
        let model = self.model();
        let operation = op.to_string();
        let lhs = self.eval(left, frame)?;
        if op == BinaryOp::Implies && boolean(model, &lhs, &operation)? == Some(false) {
            return Ok(vec![Item::Boolean(true)]);
        }
        let rhs = self.eval(right, frame)?;
        match op {
            BinaryOp::And | BinaryOp::Or | BinaryOp::Xor | BinaryOp::Implies => {
                let (a, b) = (
                    boolean(model, &lhs, &operation)?,
                    boolean(model, &rhs, &operation)?,
                );
                Ok(truth(logic(op, a, b)))
            }
            BinaryOp::Equal => Ok(truth(collections_equal(model, &lhs, &rhs)?)),
            BinaryOp::NotEqual => Ok(truth(
                collections_equal(model, &lhs, &rhs)?.map(|equal| !equal),
            )),
            BinaryOp::Equivalent => Ok(vec![Item::Boolean(collections_equivalent(
                model, &lhs, &rhs,
            )?)]),
            BinaryOp::NotEquivalent => Ok(vec![Item::Boolean(!collections_equivalent(
                model, &lhs, &rhs,
            )?)]),
            BinaryOp::Union => {
                let mut all = lhs;
                all.extend(rhs);
                distinct(model, all)
            }
            BinaryOp::In => membership(model, &lhs, &rhs, "in"),
            BinaryOp::Contains => membership(model, &rhs, &lhs, "contains"),
            BinaryOp::Concatenate => {
                let text = |items: &Collection<'a>| -> Result<String, EvalError> {
                    match single_value(model, items, "&")? {
                        None => Ok(String::new()),
                        Some(Item::String(text)) => Ok(text),
                        Some(other) => Err(EvalError::type_error("&", describe(&other))),
                    }
                };
                Ok(vec![Item::String(text(&lhs)? + &text(&rhs)?)])
            }
            BinaryOp::Less
            | BinaryOp::LessOrEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterOrEqual => {
                let (Some(a), Some(b)) = (
                    single_value(model, &lhs, &operation)?,
                    single_value(model, &rhs, &operation)?,
                ) else {
                    return Ok(Vec::new());
                };
                let order = compare(&a, &b, &operation)?;
                Ok(truth(order.map(|order| match op {
                    BinaryOp::Less => order == Ordering::Less,
                    BinaryOp::LessOrEqual => order != Ordering::Greater,
                    BinaryOp::Greater => order == Ordering::Greater,
                    _ => order != Ordering::Less,
                })))
            }
            BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Div
            | BinaryOp::Mod
            | BinaryOp::Add
            | BinaryOp::Subtract => {
                let (Some(a), Some(b)) = (
                    single_value(model, &lhs, &operation)?,
                    single_value(model, &rhs, &operation)?,
                ) else {
                    return Ok(Vec::new());
                };
                arithmetic(op, &a, &b)
            }
        }
    }
}

/// The three-valued logic of N1 §6.5.
fn logic(op: BinaryOp, a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match op {
        BinaryOp::And => match (a, b) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        },
        BinaryOp::Or => match (a, b) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        },
        BinaryOp::Xor => match (a, b) {
            (Some(a), Some(b)) => Some(a != b),
            _ => None,
        },
        _ => match (a, b) {
            (Some(true), b) => b,
            (Some(false), _) | (None, Some(true)) => Some(true),
            (None, _) => None,
        },
    }
}

/// `in` (N1 §6.4.2), with `contains` as its converse: the single `item` of
/// one side looked up in the other.
fn membership<'a>(
    model: &dyn Model,
    item: &Collection<'a>,
    collection: &Collection<'a>,
    operation: &str,
) -> Result<Collection<'a>, EvalError> {
    let item = match item.as_slice() {
        [] => return Ok(Vec::new()),
        [item] => item,
        items => return Err(EvalError::singleton(operation, items.len())),
    };
    Ok(vec![Item::Boolean(contains(model, collection, item)?)])
}

/// Orders two system values of comparable types, `None` when unknown (N1
/// §6.2).
pub(crate) fn compare(
    a: &Item<'_>,
    b: &Item<'_>,
    operation: &str,
) -> Result<Option<Ordering>, EvalError> {
    if let (Item::String(a), Item::String(b)) = (a, b) {
        return Ok(Some(a.cmp(b)));
    }
    if let (Some(a), Some(b)) = (number(a), number(b)) {
        return Ok(Some(a.cmp(&b)));
    }
    if let (Item::Time(a), Item::Time(b)) = (a, b) {
        return Ok(a.compare(b));
    }
    if let (Item::Date(a), Item::Date(b)) = (a, b) {
        return Ok(a.compare(*b));
    }
    if let (Some(a), Some(b)) = (date_time(a), date_time(b)) {
        return Ok(a.compare(&b));
    }
    if matches!((a, b), (Item::Quantity(_), _) | (_, Item::Quantity(_)))
        && let (Some(a), Some(b)) = (quantity(a), quantity(b))
    {
        return Ok(a.compare(&b));
    }
    Err(EvalError::type_error(
        operation,
        format!("{} and {}", describe(a), describe(b)),
    ))
}

/// The arithmetic operators of N1 §6.6 and date and time arithmetic (§6.7).
fn arithmetic<'a>(op: BinaryOp, a: &Item<'a>, b: &Item<'a>) -> Result<Collection<'a>, EvalError> {
    let operation = op.to_string();
    let mismatch =
        || EvalError::type_error(&operation, format!("{} and {}", describe(a), describe(b)));
    if let (Item::Integer(x), Item::Integer(y)) = (a, b) {
        let result = match op {
            BinaryOp::Add => integer(x.checked_add(*y)),
            BinaryOp::Subtract => integer(x.checked_sub(*y)),
            BinaryOp::Multiply => integer(x.checked_mul(*y)),
            BinaryOp::Div => integer(x.checked_div(*y)),
            BinaryOp::Mod => integer(x.checked_rem(*y)),
            _ => Decimal::from(*x)
                .checked_div(Decimal::from(*y))
                .map(Item::Decimal),
        };
        return Ok(result.into_iter().collect());
    }
    if let (Some(x), Some(y)) = (number(a), number(b)) {
        let result = match op {
            BinaryOp::Add => x.checked_add(y),
            BinaryOp::Subtract => x.checked_sub(y),
            BinaryOp::Multiply => x.checked_mul(y),
            BinaryOp::Divide => x.checked_div(y),
            BinaryOp::Div => x.checked_div(y).map(|q| q.trunc()),
            _ => x.checked_rem(y),
        };
        return Ok(result.map(Item::Decimal).into_iter().collect());
    }
    if let (Item::String(x), Item::String(y)) = (a, b) {
        return match op {
            BinaryOp::Add => Ok(vec![Item::String(format!("{x}{y}"))]),
            _ => Err(mismatch()),
        };
    }
    if let Item::Quantity(q) = b
        && matches!(a, Item::Date(_) | Item::DateTime(_) | Item::Time(_))
        && matches!(op, BinaryOp::Add | BinaryOp::Subtract)
    {
        return moved(a, q, op == BinaryOp::Subtract).map(|item| vec![item]);
    }
    if matches!((a, b), (Item::Quantity(_), _) | (_, Item::Quantity(_)))
        && let (Some(x), Some(y)) = (quantity(a), quantity(b))
    {
        let result = match op {
            BinaryOp::Add => x.add(&y, false),
            BinaryOp::Subtract => x.add(&y, true),
            BinaryOp::Multiply => x.multiply(&y),
            BinaryOp::Divide => x.divide(&y),
            _ => return Err(mismatch()),
        };
        return Ok(result.map(Item::Quantity).into_iter().collect());
    }
    Err(mismatch())
}

/// A date, datetime or time moved by a time-valued quantity (N1 §6.7).
///
/// NOTE: N1 §6.7 admits "an equivalent unit" without listing it; the definite units of a
/// fixed length (`'wk'`, `'d'`, `'h'`, `'min'`, `'s'`, `'ms'`) are taken and `'mo'` and `'a'` refused.
fn moved<'a>(
    value: &Item<'a>,
    quantity: &crate::quantity::Quantity,
    subtract: bool,
) -> Result<Item<'a>, EvalError> {
    let unit = match &quantity.unit {
        Unit::Calendar(calendar) => *calendar,
        Unit::Ucum(code) => match code.as_str() {
            "wk" => Calendar::Week,
            "d" => Calendar::Day,
            "h" => Calendar::Hour,
            "min" => Calendar::Minute,
            "s" => Calendar::Second,
            "ms" => Calendar::Millisecond,
            _ => {
                return Err(EvalError::type_error(
                    "date and time arithmetic",
                    format!("a quantity in '{code}'"),
                ));
            }
        },
    };
    let amount = if subtract {
        -quantity.value
    } else {
        quantity.value
    };
    let arithmetic = |source| EvalError::Arithmetic { source };
    Ok(match value {
        Item::Date(date) => Item::Date(date.add(amount, unit).map_err(arithmetic)?),
        Item::DateTime(date_time) => {
            Item::DateTime(date_time.add(amount, unit).map_err(arithmetic)?)
        }
        Item::Time(time) => Item::Time(time.add(amount, unit).map_err(arithmetic)?),
        other => {
            return Err(EvalError::type_error(
                "date and time arithmetic",
                describe(other),
            ));
        }
    })
}
