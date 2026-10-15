// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The conversions every operator and function shares: a FHIR primitive to
//! its system value (FHIR R4 §2.9.1.2), a FHIR `Quantity` to
//! `System.Quantity` (§2.9.1.3), and singleton evaluation (N1 §4.5).

use fhir_types::codec::Value;
use fhir_types::schema::ValueKind;
use rust_decimal::Decimal;

use super::error::EvalError;
use crate::model::{Model, TypeKind, is_a};
use crate::quantity::{Calendar, Quantity, Unit};
use crate::temporal::{DateTime, Time};
use crate::value::node::Node;
use crate::value::{Collection, Item};

/// The UCUM system url (<https://hl7.org/fhir/R4/ucum.html>).
pub const UCUM: &str = "http://unitsofmeasure.org";

/// The system value a FHIR primitive carries, `None` when it has only an id
/// or extensions.
///
/// The FHIR primitive types map onto the system types by the table of FHIR R4
/// §2.9.1.2: `date`, `dateTime` and `instant` are `System.DateTime`, `time`
/// is `System.Time`, the numbers are `System.Integer` or `System.Decimal`,
/// and every other primitive is a `System.String`.
///
/// # Errors
///
/// Returns [`EvalError::Document`] when the JSON value is not of the
/// primitive's kind, or a date or time is not in its lexical form.
pub fn primitive_value<'a>(node: &Node<'_>) -> Result<Option<Item<'a>>, EvalError> {
    let Some(ty) = node.fhir_type() else {
        return Ok(None);
    };
    let Some(json) = node.json() else {
        return Ok(None);
    };
    let malformed = || EvalError::Document {
        element: String::from(node.definition().unwrap_or(ty.name)),
        reason: format!("the value is not a {}", ty.name),
    };
    let value = match (ty.kind, json) {
        (TypeKind::Primitive(ValueKind::Boolean), Value::Bool(flag)) => Item::Boolean(*flag),
        (TypeKind::Primitive(ValueKind::Integer), Value::Number(number)) => {
            Item::Integer(number.as_i64().ok_or_else(malformed)?)
        }
        (TypeKind::Primitive(ValueKind::Decimal), Value::Number(number)) => {
            Item::Decimal(decimal(number.as_str()).ok_or_else(malformed)?)
        }
        (TypeKind::Primitive(ValueKind::Text) | TypeKind::Xhtml, Value::String(text)) => {
            match ty.name {
                "date" | "dateTime" | "instant" => {
                    Item::DateTime(DateTime::parse(text).ok_or_else(malformed)?)
                }
                "time" => Item::Time(Time::parse(text).ok_or_else(malformed)?),
                _ => Item::String(text.clone()),
            }
        }
        _ => return Err(malformed()),
    };
    Ok(Some(value))
}

/// A decimal in its JSON lexical form, exponent included.
fn decimal(text: &str) -> Option<Decimal> {
    text.parse::<Decimal>()
        .ok()
        .or_else(|| Decimal::from_scientific(text).ok())
}

/// The `System.Quantity` a FHIR `Quantity` maps to: only one with a UCUM
/// code, its time units mapped to the calendar keywords (FHIR R4 §2.9.1.3).
///
/// # Errors
///
/// Returns [`EvalError::Document`] when the quantity's value or code
/// contradicts the model.
pub fn fhir_quantity(model: &dyn Model, node: &Node<'_>) -> Result<Option<Quantity>, EvalError> {
    let Some(ty) = node.fhir_type() else {
        return Ok(None);
    };
    if !is_a(model, ty, "Quantity") {
        return Ok(None);
    }
    let Some(json) = node.json() else {
        return Ok(None);
    };
    let text = |key: &str| json.get(key).and_then(Value::as_str);
    let (Some(UCUM), Some(code)) = (text("system"), text("code")) else {
        return Ok(None);
    };
    let Some(Value::Number(number)) = json.get("value") else {
        return Ok(None);
    };
    let value = decimal(number.as_str()).ok_or_else(|| EvalError::Document {
        element: String::from("Quantity.value"),
        reason: String::from("the value is not a decimal"),
    })?;
    let unit = match code {
        "a" => Unit::Calendar(Calendar::Year),
        "mo" => Unit::Calendar(Calendar::Month),
        "d" => Unit::Calendar(Calendar::Day),
        "h" => Unit::Calendar(Calendar::Hour),
        "min" => Unit::Calendar(Calendar::Minute),
        "s" => Unit::Calendar(Calendar::Second),
        other => Unit::Ucum(String::from(other)),
    };
    Ok(Some(Quantity::new(value, unit)))
}

/// The item as a system value where it converts to one.
///
/// A FHIR primitive becomes its value and a FHIR `Quantity` with a UCUM code
/// a `System.Quantity`; any other item stays as it is. `None` for a primitive
/// with no value.
///
/// # Errors
///
/// Returns [`EvalError::Document`] when the document contradicts the model.
pub fn system<'a>(model: &dyn Model, item: &Item<'a>) -> Result<Option<Item<'a>>, EvalError> {
    let Item::Node(node) = item else {
        return Ok(Some(item.clone()));
    };
    match node.fhir_type() {
        Some(ty) if ty.is_primitive() => primitive_value(node),
        Some(_) => match fhir_quantity(model, node)? {
            Some(quantity) => Ok(Some(Item::Quantity(quantity))),
            None => Ok(Some(item.clone())),
        },
        None => Ok(Some(item.clone())),
    }
}

/// The single item of a collection: `None` when it is empty (N1 §4.5).
///
/// # Errors
///
/// Returns [`EvalError::Singleton`] when it holds more than one item.
pub fn single<'c, 'a>(
    collection: &'c Collection<'a>,
    operation: &str,
) -> Result<Option<&'c Item<'a>>, EvalError> {
    match collection.as_slice() {
        [] => Ok(None),
        [item] => Ok(Some(item)),
        items => Err(EvalError::singleton(operation, items.len())),
    }
}

/// The single item as a system value, `None` for an empty collection or a
/// primitive with no value (N1 §4.5).
///
/// # Errors
///
/// Returns [`EvalError::Singleton`] for more than one item, and what
/// [`system`] returns.
pub fn single_value<'a>(
    model: &dyn Model,
    collection: &Collection<'a>,
    operation: &str,
) -> Result<Option<Item<'a>>, EvalError> {
    match single(collection, operation)? {
        Some(item) => system(model, item),
        None => Ok(None),
    }
}

/// The collection as a Boolean (N1 §4.5): empty is `None`, one Boolean is
/// its value, and any other single item is `true`.
///
/// # Errors
///
/// Returns [`EvalError::Singleton`] for more than one item.
pub fn boolean(
    model: &dyn Model,
    collection: &Collection<'_>,
    operation: &str,
) -> Result<Option<bool>, EvalError> {
    match single_value(model, collection, operation)? {
        None => Ok(match collection.as_slice() {
            [] => None,
            _ => Some(true),
        }),
        Some(Item::Boolean(flag)) => Ok(Some(flag)),
        Some(_) => Ok(Some(true)),
    }
}

/// The collection as one string, `None` when empty.
///
/// # Errors
///
/// Returns [`EvalError::Singleton`] for more than one item and
/// [`EvalError::Type`] for an item that is no string.
pub fn string(
    model: &dyn Model,
    collection: &Collection<'_>,
    operation: &str,
) -> Result<Option<String>, EvalError> {
    match single_value(model, collection, operation)? {
        None => Ok(None),
        Some(Item::String(text)) => Ok(Some(text)),
        Some(other) => Err(EvalError::type_error(operation, describe(&other))),
    }
}

/// The collection as one integer, `None` when empty.
///
/// # Errors
///
/// Returns [`EvalError::Singleton`] for more than one item and
/// [`EvalError::Type`] for an item that is no integer.
pub fn integer(
    model: &dyn Model,
    collection: &Collection<'_>,
    operation: &str,
) -> Result<Option<i64>, EvalError> {
    match single_value(model, collection, operation)? {
        None => Ok(None),
        Some(Item::Integer(value)) => Ok(Some(value)),
        Some(other) => Err(EvalError::type_error(operation, describe(&other))),
    }
}

/// A short description of an item's type for an error.
#[must_use]
pub fn describe(item: &Item<'_>) -> String {
    match item {
        Item::Node(node) => node.to_string(),
        Item::Type(_) => String::from("System.TypeInfo"),
        other => format!("System.{}", other.system_type().unwrap_or("Any")),
    }
}
