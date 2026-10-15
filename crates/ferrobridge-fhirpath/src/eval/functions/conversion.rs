// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `iif()` and the conversion functions (N1 §5.5).

use std::sync::LazyLock;

use regex::Regex;
use rust_decimal::Decimal;

use super::Call;
use crate::eval::convert::{boolean, single_value, string};
use crate::eval::error::EvalError;
use crate::quantity::{Quantity, Unit};
use crate::temporal::{Date, DateTime, Time};
use crate::value::{Collection, Item};

/// `(\+|-)?\d+`, the integer form of N1 §5.5.3.
static INTEGER: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"^(\+|-)?\d+$").ok());

/// `(\+|-)?\d+(\.\d+)?`, the decimal form of N1 §5.5.6.
static DECIMAL: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^(\+|-)?\d+(\.\d+)?$").ok());

/// Whether `text` matches one of the forms above.
fn matches(form: &LazyLock<Option<Regex>>, text: &str) -> bool {
    form.as_ref().is_some_and(|form| form.is_match(text))
}

/// Calls the function when this module defines it.
pub(super) fn call<'a>(call: &Call<'_, '_, 'a>) -> Result<Option<Collection<'a>>, EvalError> {
    if call.name == "iif" {
        return iif(call).map(Some);
    }
    let Some((target, test)) = target(call.name) else {
        return Ok(None);
    };
    let arity = if target == Target::Quantity {
        (0, 1, "0 or 1")
    } else {
        (0, 0, "no")
    };
    call.arity(arity.0, arity.1, arity.2)?;
    let Some(item) = single_value(call.model(), &call.input, call.name)? else {
        return Ok(Some(Vec::new()));
    };
    let unit = match call.arg(0)? {
        Some(unit) => string(call.model(), &unit, call.name)?,
        None => None,
    };
    let converted = convert(&item, target, unit.as_deref());
    Ok(Some(if test {
        vec![Item::Boolean(converted.is_some())]
    } else {
        converted.into_iter().collect()
    }))
}

/// `iif(criterion, true-result [, otherwise-result])` (N1 §5.5.1), which
/// evaluates only the branch the criterion selects.
fn iif<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    call.arity(2, 3, "2 or 3")?;
    let focus = match call.input.as_slice() {
        [] => None,
        [item] => Some(item.clone()),
        items => return Err(EvalError::singleton("iif", items.len())),
    };
    // NOTE: N1 §5.5.1 leaves the input of iif() open; no specification governs this: our own
    // design, its single input item is `$this` while `$index` and `$total` stay those around it.
    let frame = crate::eval::Frame {
        focus: focus.into_iter().collect(),
        index: call.frame.index,
        total: call.frame.total.clone(),
    };
    let evaluate = |index: usize| {
        call.args
            .get(index)
            .map_or_else(|| Ok(Vec::new()), |arg| call.engine.eval(arg, &frame))
    };
    if boolean(call.model(), &evaluate(0)?, "iif")? == Some(true) {
        evaluate(1)
    } else if call.args.len() == 3 {
        evaluate(2)
    } else {
        Ok(Vec::new())
    }
}

/// The type a conversion function converts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Boolean,
    Integer,
    Decimal,
    Date,
    DateTime,
    Time,
    Quantity,
    String,
}

/// The target of `toX()` and `convertsToX()`, with whether the function only
/// tests the conversion.
fn target(name: &str) -> Option<(Target, bool)> {
    let (rest, test) = match name.strip_prefix("convertsTo") {
        Some(rest) => (rest, true),
        None => (name.strip_prefix("to")?, false),
    };
    let target = match rest {
        "Boolean" => Target::Boolean,
        "Integer" => Target::Integer,
        "Decimal" => Target::Decimal,
        "Date" => Target::Date,
        "DateTime" => Target::DateTime,
        "Time" => Target::Time,
        "Quantity" => Target::Quantity,
        "String" => Target::String,
        _ => return None,
    };
    Some((target, test))
}

/// The item converted to `target` by the table of N1 §5.5, `None` when it
/// does not convert.
fn convert<'a>(item: &Item<'_>, target: Target, unit: Option<&str>) -> Option<Item<'a>> {
    match target {
        Target::Boolean => to_boolean(item).map(Item::Boolean),
        Target::Integer => match item {
            Item::Integer(value) => Some(Item::Integer(*value)),
            Item::Boolean(value) => Some(Item::Integer(i64::from(*value))),
            Item::String(text) if matches(&INTEGER, text) => text
                .trim_start_matches('+')
                .parse::<i32>()
                .ok()
                .map(|value| Item::Integer(i64::from(value))),
            _ => None,
        },
        Target::Decimal => match item {
            Item::Integer(value) => Some(Item::Decimal(Decimal::from(*value))),
            Item::Decimal(value) => Some(Item::Decimal(*value)),
            Item::Boolean(value) => {
                Some(Item::Decimal(Decimal::new(if *value { 10 } else { 0 }, 1)))
            }
            Item::String(text) if matches(&DECIMAL, text) => {
                text.trim_start_matches('+').parse().ok().map(Item::Decimal)
            }
            _ => None,
        },
        Target::Date => match item {
            Item::Date(value) => Some(Item::Date(*value)),
            Item::DateTime(value) => Some(Item::Date(value.date())),
            Item::String(text) => Date::parse(text).map(Item::Date),
            _ => None,
        },
        Target::DateTime => match item {
            Item::DateTime(value) => Some(Item::DateTime(*value)),
            Item::Date(value) => Some(Item::DateTime(value.to_date_time())),
            Item::String(text) => DateTime::parse(text).map(Item::DateTime),
            _ => None,
        },
        Target::Time => match item {
            Item::Time(value) => Some(Item::Time(*value)),
            Item::String(text) => Time::parse(text).map(Item::Time),
            _ => None,
        },
        Target::Quantity => to_quantity(item)
            .and_then(|quantity| in_unit(quantity, unit))
            .map(Item::Quantity),
        Target::String => match item {
            Item::Node(_) | Item::Type(_) => None,
            other => Some(Item::String(other.to_string())),
        },
    }
}

/// The Boolean an item converts to (N1 §5.5.2).
fn to_boolean(item: &Item<'_>) -> Option<bool> {
    match item {
        Item::Boolean(value) => Some(*value),
        Item::Integer(1) => Some(true),
        Item::Integer(0) => Some(false),
        Item::Decimal(value) if *value == Decimal::ONE => Some(true),
        Item::Decimal(value) if value.is_zero() => Some(false),
        Item::String(text) => match text.to_lowercase().as_str() {
            "true" | "t" | "yes" | "y" | "1" | "1.0" => Some(true),
            "false" | "f" | "no" | "n" | "0" | "0.0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// The quantity an item converts to (N1 §5.5.7).
fn to_quantity(item: &Item<'_>) -> Option<Quantity> {
    match item {
        Item::Integer(value) => Some(Quantity::new(Decimal::from(*value), Unit::one())),
        Item::Decimal(value) => Some(Quantity::new(*value, Unit::one())),
        Item::Quantity(value) => Some(value.clone()),
        Item::Boolean(value) => Some(Quantity::new(
            Decimal::new(if *value { 10 } else { 0 }, 1),
            Unit::one(),
        )),
        Item::String(text) => Quantity::parse(text),
        _ => None,
    }
}

/// The quantity in `unit` when one is asked for: the same unit, or a
/// calendar duration converted by N1 §5.5.7's factors; `None` otherwise, as
/// N1 permits for a unit different from the input's.
fn in_unit(quantity: Quantity, unit: Option<&str>) -> Option<Quantity> {
    let Some(unit) = unit else {
        return Some(quantity);
    };
    let wanted = crate::quantity::Calendar::from_keyword(unit)
        .map_or_else(|| Unit::Ucum(String::from(unit)), Unit::Calendar);
    if quantity.unit == wanted {
        return Some(quantity);
    }
    match (&quantity.unit, &wanted) {
        (Unit::Calendar(from), Unit::Calendar(to)) => {
            let value = from.convert(quantity.value, *to)?;
            Some(Quantity::new(value, wanted))
        }
        // TODO(#459): convert between different UCUM units.
        _ => None,
    }
}
