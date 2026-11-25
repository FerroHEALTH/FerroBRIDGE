// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The target transforms of FHIR R4 §7.7.0.7.2: how a target value is made
//! from literals and variables.
//!
//! `translate` calls a terminology operation and `dateOp` and `escape` are
//! left undocumented by R4, so each answers a typed refusal.

use fhir_types::codec::{Number, Object, Value};

use super::{Bound, MapError, Run, Scope};
use crate::structuremap::syntax::{Invocation, Literal, Param, Transform};

/// Returns the value `literal` writes.
pub(super) fn literal(literal: &Literal) -> Value {
    match literal {
        Literal::Integer(text) | Literal::Decimal(text) => text
            .parse::<Number>()
            .map_or_else(|_lexical| Value::String(text.clone()), Value::Number),
        Literal::String(text) | Literal::DateTime(text) | Literal::Time(text) => {
            Value::String(text.clone())
        }
        Literal::Boolean(flag) => Value::Bool(*flag),
    }
}

/// Returns the type a `create` names.
pub(super) fn type_parameter(invocation: &Invocation) -> Result<String, MapError> {
    match invocation.params.as_slice() {
        [Param::Literal(Literal::String(type_name)) | Param::Variable(type_name)] => {
            Ok(type_name.clone())
        }
        _ => Err(refused("create", "takes one type name")),
    }
}

/// Returns the refusal of `name` for `reason`.
fn refused(name: &str, reason: &'static str) -> MapError {
    MapError::Transform {
        name: name.to_owned(),
        reason,
    }
}

/// Returns the value `transform` makes in `scope`; `None` when an
/// expression answers nothing.
pub(super) fn apply(
    run: &Run<'_>,
    transform: &Transform,
    scope: &Scope,
) -> Result<Option<Value>, MapError> {
    match transform {
        Transform::Literal(constant) => Ok(Some(literal(constant))),
        Transform::Copy { context, element } => {
            let bound = scope
                .get(context)
                .ok_or_else(|| MapError::UnknownVariable(context.clone()))?;
            let (value, _) = run.value_of(bound);
            Ok(match element {
                Some(element) => value.get(element).cloned(),
                None => Some(value),
            })
        }
        Transform::Evaluate(expression) => single(run.evaluate(&expression.0, None, scope)?),
        Transform::Invocation(invocation) => invoke(run, invocation, scope),
    }
}

/// Returns the one value of `values`, `None` for none.
fn single(values: Vec<Value>) -> Result<Option<Value>, MapError> {
    let mut values = values.into_iter();
    match (values.next(), values.next()) {
        (value, None) => Ok(value),
        _ => Err(refused("evaluate", "answers more than one value")),
    }
}

/// Returns the values of `invocation`'s parameters: a variable's current
/// value, or a literal; a name no variable holds is a string, as a type name
/// is written.
fn parameters(run: &Run<'_>, invocation: &Invocation, scope: &Scope) -> Vec<Value> {
    invocation
        .params
        .iter()
        .map(|param| match param {
            Param::Literal(constant) => literal(constant),
            Param::Variable(name) => scope.get(name).map_or_else(
                || Value::String(name.clone()),
                |bound: &Bound| run.value_of(bound).0,
            ),
        })
        .collect()
}

/// Returns the text of a primitive `value`.
fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.as_str().to_owned()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// Returns an object of the members `pairs` names, skipping absent ones.
fn object(pairs: &[(&str, Option<Value>)]) -> Value {
    let mut object = Object::new();
    for (name, value) in pairs {
        if let Some(value) = value {
            object.insert((*name).to_owned(), value.clone());
        }
    }
    Value::Object(object)
}

/// Returns the value the named transform `invocation` makes.
pub(super) fn invoke(
    run: &Run<'_>,
    invocation: &Invocation,
    scope: &Scope,
) -> Result<Option<Value>, MapError> {
    let name = invocation.name.as_str();
    let values = parameters(run, invocation, scope);
    if let Some(value) = datatype(name, &values)? {
        return Ok(Some(value));
    }
    let string = |index: usize| values.get(index).and_then(text);
    Ok(Some(match (name, values.as_slice()) {
        ("copy", [value]) => value.clone(),
        ("truncate", [value, length]) => {
            let length = length
                .as_str()
                .map(str::to_owned)
                .or_else(|| text(length))
                .and_then(|length| length.parse::<usize>().ok())
                .ok_or_else(|| refused(name, "takes a length"))?;
            let source = text(value).ok_or_else(|| refused(name, "takes a primitive"))?;
            Value::String(source.chars().take(length).collect())
        }
        ("append", parts) if !parts.is_empty() => Value::String(
            parts
                .iter()
                .map(|part| text(part).ok_or_else(|| refused(name, "takes primitives")))
                .collect::<Result<String, _>>()?,
        ),
        ("cast", [value, ..]) => {
            let source = text(value).ok_or_else(|| refused(name, "takes a primitive"))?;
            match string(1).as_deref() {
                None | Some("string") => Value::String(source),
                Some("integer" | "decimal") => source
                    .parse::<Number>()
                    .map(Value::Number)
                    .map_err(|_lexical| refused(name, "reads no number"))?,
                Some("boolean") => match source.as_str() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    _ => return Err(refused(name, "reads no boolean")),
                },
                Some(_) => return Err(refused(name, "names a type it cannot cast to")),
            }
        }
        ("uuid", []) => Value::String(uuid::Uuid::new_v4().to_string()),
        ("reference" | "pointer", [resource]) => {
            let type_name = resource.get("resourceType").and_then(Value::as_str);
            let id = resource.get("id").and_then(Value::as_str);
            match (type_name, id) {
                (Some(type_name), Some(id)) => Value::String(format!("{type_name}/{id}")),
                _ => return Err(refused(name, "takes a resource with an id")),
            }
        }
        ("evaluate", [focus, Value::String(expression)]) => {
            return single(run.evaluate(expression, Some(focus), scope)?);
        }
        // TODO(#2118): translate through the terminology server's ConceptMap/$translate.
        ("translate" | "dateOp" | "escape", _) => {
            return Err(refused(name, "is not run by this engine"));
        }
        _ => return Err(refused(name, "is unknown or takes other parameters")),
    }))
}

/// Returns the datatype the transform `name` builds from `values` (`cc`,
/// `c`, `qty`, `id` and `cp`), `None` for any other transform.
fn datatype(name: &str, values: &[Value]) -> Result<Option<Value>, MapError> {
    let string = |index: usize| values.get(index).and_then(text);
    let string_value = |index: usize| string(index).map(Value::String);
    Ok(Some(match (name, values) {
        ("cc", [_]) => object(&[("text", string_value(0))]),
        ("cc", [_, _] | [_, _, _]) => object(&[(
            "coding",
            Some(Value::Array(vec![object(&[
                ("system", string_value(0)),
                ("code", string_value(1)),
                ("display", string_value(2)),
            ])])),
        )]),
        ("c", [_, _] | [_, _, _]) => object(&[
            ("system", string_value(0)),
            ("code", string_value(1)),
            ("display", string_value(2)),
        ]),
        ("qty", [value, _] | [value, _, _, _]) => object(&[
            (
                "value",
                Some(
                    text(value)
                        .and_then(|text| text.parse::<Number>().ok())
                        .map(Value::Number)
                        .ok_or_else(|| refused(name, "takes a numeric value"))?,
                ),
            ),
            ("unit", string_value(1)),
            ("system", string_value(2)),
            ("code", string_value(3)),
        ]),
        ("id", [_, _] | [_, _, _]) => object(&[
            ("system", string_value(0)),
            ("value", string_value(1)),
            (
                "type",
                string(2).map(|code| {
                    // NOTE: R4 valueset-identifier-type; the identifier type codes are v2 table 0203.
                    object(&[(
                        "coding",
                        Some(Value::Array(vec![object(&[
                            (
                                "system",
                                Some(Value::String(String::from(
                                    "http://terminology.hl7.org/CodeSystem/v2-0203",
                                ))),
                            ),
                            ("code", Some(Value::String(code))),
                        ])])),
                    )])
                }),
            ),
        ]),
        ("cp", [value]) => {
            let value = text(value).ok_or_else(|| refused(name, "takes a primitive"))?;
            let system = if value.contains('@') {
                "email"
            } else if value.starts_with("http://") || value.starts_with("https://") {
                "url"
            } else {
                "phone"
            };
            object(&[
                ("system", Some(Value::String(String::from(system)))),
                ("value", Some(Value::String(value))),
            ])
        }
        ("cp", [_, _]) => object(&[("system", string_value(0)), ("value", string_value(1))]),
        _ => return Ok(None),
    }))
}
