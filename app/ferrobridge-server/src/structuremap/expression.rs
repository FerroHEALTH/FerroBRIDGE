// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIRPath a map embeds, evaluated by `ferrobridge-fhirpath` over the
//! map's variables.
//!
//! A map names its variables bare (`where a.length() <= 20`, FHIR R4
//! §7.7.0.7.1), and FHIRPath names an environment variable with `%`
//! (FHIRPath N1 §2.9.1), so each bare identifier that names a variable and
//! is no member or function name is read as that variable. A logical-model
//! instance carries no FHIR type, so its values are untyped nodes, whose
//! members are read from the JSON.

use ferrobridge_fhirpath::eval::{Environment, Focus};
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::model::r4::R4;
use ferrobridge_fhirpath::value::{Collection, Item};
use fhir_types::codec::{Number, Object, Value};

/// Why an embedded expression failed, naming no value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ExpressionError {
    /// The expression does not parse.
    #[error("the FHIRPath expression `{0}` does not parse")]
    Parse(String),
    /// The evaluation failed.
    #[error("the FHIRPath expression `{0}` failed")]
    Evaluate(String),
    /// The result holds an item no JSON value carries.
    #[error("the FHIRPath expression `{0}` answered an item no element holds")]
    Result(String),
}

/// Returns `text` with every bare identifier that is one of `variables`
/// written as `%identifier`.
fn bind(text: &str, variables: &[&str]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut previous: Option<char> = None;
    while let Some(c) = rest.chars().next() {
        if c == '\'' || c == '`' || c == '"' {
            let length = rest
                .char_indices()
                .skip(1)
                .scan(false, |escaped, (index, current)| {
                    let end = !*escaped && current == c;
                    *escaped = !*escaped && current == '\\';
                    Some((index, end))
                })
                .find(|(_, end)| *end)
                .map_or(rest.len(), |(index, _)| index + c.len_utf8());
            out.push_str(rest.get(..length).unwrap_or(rest));
            previous = Some(c);
            rest = rest.get(length..).unwrap_or_default();
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let length = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            let word = rest.get(..length).unwrap_or_default();
            let after = rest.get(length..).unwrap_or_default().trim_start();
            let member = matches!(previous, Some('.' | '%' | '$'));
            if !member && !after.starts_with('(') && variables.contains(&word) {
                out.push('%');
            }
            out.push_str(word);
            previous = word.chars().last();
            rest = rest.get(length..).unwrap_or_default();
            continue;
        }
        out.push(c);
        if !c.is_whitespace() {
            previous = Some(c);
        }
        rest = rest.get(c.len_utf8()..).unwrap_or_default();
    }
    out
}

/// Evaluates `text` on `focus` with `variables` bound by name, and returns
/// its result as JSON values.
///
/// # Errors
/// Returns [`ExpressionError`] when the expression does not parse, fails,
/// or answers an item no JSON value carries.
pub fn evaluate(
    text: &str,
    focus: Option<&Value>,
    variables: &[(&str, &Value)],
) -> Result<Vec<Value>, ExpressionError> {
    let names: Vec<&str> = variables.iter().map(|(name, _)| *name).collect();
    let expression = Expression::parse(&bind(text, &names))
        .map_err(|_parse| ExpressionError::Parse(text.to_owned()))?;
    let member =
        Expression::parse("v").map_err(|_parse| ExpressionError::Parse(text.to_owned()))?;
    let wrap = |value: &Value| {
        let mut object = Object::new();
        object.insert(String::from("v"), value.clone());
        Value::Object(object)
    };
    let holders: Vec<(&str, Value)> = variables
        .iter()
        .map(|(name, value)| (*name, wrap(value)))
        .collect();
    let focus_holder = focus.map(wrap);
    let mut environment = Environment::new(&R4);
    for (name, holder) in &holders {
        environment = environment.with_variable(*name, items(&member, holder, text)?);
    }
    let context = match &focus_holder {
        Some(holder) => Focus::items(items(&member, holder, text)?, None),
        None => Focus::none(),
    };
    let result = expression
        .evaluate(&environment, context)
        .map_err(|_failed| ExpressionError::Evaluate(text.to_owned()))?;
    result
        .iter()
        .map(|item| to_value(item).ok_or_else(|| ExpressionError::Result(text.to_owned())))
        .collect()
}

/// Returns the items the member `v` of `holder` reads.
fn items<'a>(
    member: &Expression,
    holder: &'a Value,
    text: &str,
) -> Result<Collection<'a>, ExpressionError> {
    member
        .evaluate(&Environment::new(&R4), Focus::resource(holder))
        .map_err(|_failed| ExpressionError::Evaluate(text.to_owned()))
}

/// Returns `item` as a JSON value.
fn to_value(item: &Item<'_>) -> Option<Value> {
    Some(match item {
        Item::Boolean(flag) => Value::Bool(*flag),
        Item::String(text) => Value::String(text.clone()),
        Item::Integer(number) => Value::Number(Number::from(*number)),
        Item::Decimal(number) => Value::Number(number.to_string().parse().ok()?),
        Item::Date(date) => Value::String(date.to_string()),
        Item::DateTime(instant) => Value::String(instant.to_string()),
        Item::Time(time) => Value::String(time.to_string()),
        Item::Node(node) => node.json()?.clone(),
        _ => return None,
    })
}

/// Returns whether `values` is the single `true` a condition needs
/// (FHIRPath N1 §5.1.1, singleton evaluation of a boolean).
#[must_use]
pub fn holds(values: &[Value]) -> bool {
    matches!(values, [Value::Bool(true)])
}

#[cfg(test)]
mod tests {
    use super::bind;

    #[test]
    fn a_bare_variable_becomes_an_environment_variable() {
        assert_eq!(
            "%a.length() <= 20 and %b = 'a' and length()",
            bind(
                "a.length() <= 20 and b = 'a' and length()",
                &["a", "b", "length"]
            )
        );
        assert_eq!("x.a = 'a b'", bind("x.a = 'a b'", &["a"]));
        assert_eq!("%m < 2", bind("m < 2", &["m"]));
    }
}
