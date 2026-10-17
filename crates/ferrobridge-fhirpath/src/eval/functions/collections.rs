// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Existence (N1 §5.1), filtering and projection (§5.2), subsetting (§5.3),
//! combining (§5.4), the type functions (§6.3) and `aggregate()` (§7).

use super::Call;
use crate::eval::convert::{boolean, describe, integer, system};
use crate::eval::equality::{contains, distinct};
use crate::eval::error::EvalError;
use crate::value::{Collection, Item};

/// Calls the function when this module defines it.
pub(super) fn call<'a>(call: &Call<'_, '_, 'a>) -> Result<Option<Collection<'a>>, EvalError> {
    let model = call.model();
    let input = &call.input;
    let flag = |value: bool| Ok(Some(vec![Item::Boolean(value)]));
    match call.name {
        "empty" => {
            call.arity(0, 0, "no")?;
            flag(input.is_empty())
        }
        "not" => {
            call.arity(0, 0, "no")?;
            Ok(Some(
                boolean(model, input, "not")?
                    .map(|value| Item::Boolean(!value))
                    .into_iter()
                    .collect(),
            ))
        }
        "exists" => {
            call.arity(0, 1, "0 or 1")?;
            if call.args.is_empty() {
                return flag(!input.is_empty());
            }
            flag(!filter(call)?.is_empty())
        }
        "all" => {
            call.arity(1, 1, "1")?;
            for (index, item) in input.iter().enumerate() {
                if boolean(model, &call.lambda(0, item, index, None)?, "all")? != Some(true) {
                    return flag(false);
                }
            }
            flag(true)
        }
        "allTrue" | "anyTrue" | "allFalse" | "anyFalse" => {
            call.arity(0, 0, "no")?;
            let mut values = Vec::with_capacity(input.len());
            for item in input {
                match system(model, item)? {
                    Some(Item::Boolean(value)) => values.push(value),
                    Some(other) => return Err(EvalError::type_error(call.name, describe(&other))),
                    None => {}
                }
            }
            flag(match call.name {
                "allTrue" => values.iter().all(|value| *value),
                "anyTrue" => values.iter().any(|value| *value),
                "allFalse" => values.iter().all(|value| !*value),
                _ => values.iter().any(|value| !*value),
            })
        }
        "subsetOf" | "supersetOf" => {
            call.arity(1, 1, "1")?;
            let other = call.arg_or_empty(0)?;
            let (part, whole) = if call.name == "subsetOf" {
                (input, &other)
            } else {
                (&other, input)
            };
            for item in part {
                if !contains(model, whole, item)? {
                    return flag(false);
                }
            }
            flag(true)
        }
        "count" => {
            call.arity(0, 0, "no")?;
            Ok(Some(vec![Item::Integer(super::length(input.len()))]))
        }
        "distinct" => {
            call.arity(0, 0, "no")?;
            distinct(model, input.clone()).map(Some)
        }
        "isDistinct" => {
            call.arity(0, 0, "no")?;
            flag(distinct(model, input.clone())?.len() == input.len())
        }
        _ => filtering(call),
    }
}

/// Filtering and projection (N1 §5.2), the type functions (§6.3) and
/// `aggregate()` (§7).
fn filtering<'a>(call: &Call<'_, '_, 'a>) -> Result<Option<Collection<'a>>, EvalError> {
    let input = &call.input;
    match call.name {
        "where" => {
            call.arity(1, 1, "1")?;
            filter(call).map(Some)
        }
        "select" => {
            call.arity(1, 1, "1")?;
            let mut out = Vec::new();
            for (index, item) in input.iter().enumerate() {
                out.extend(call.lambda(0, item, index, None)?);
            }
            Ok(Some(out))
        }
        "repeat" => {
            call.arity(1, 1, "1")?;
            repeat(call).map(Some)
        }
        "ofType" => {
            call.arity(1, 1, "1")?;
            let ty = call.engine.resolve_type(&type_argument(call)?)?;
            let mut out = Vec::new();
            for item in input {
                if call.engine.is_type(item, &ty) {
                    out.push(item.clone());
                }
            }
            Ok(Some(out))
        }
        "is" | "as" => {
            call.arity(1, 1, "1")?;
            let ty = call.engine.resolve_type(&type_argument(call)?)?;
            let item = match input.as_slice() {
                [] => return Ok(Some(Vec::new())),
                [item] => item,
                items => return Err(EvalError::singleton(call.name, items.len())),
            };
            if call.name == "is" {
                return Ok(Some(vec![Item::Boolean(
                    call.engine.is_instance(item, &ty),
                )]));
            }
            let matches = call.engine.is_type(item, &ty);
            Ok(Some(if matches {
                vec![item.clone()]
            } else {
                Vec::new()
            }))
        }
        "aggregate" => {
            call.arity(1, 2, "1 or 2")?;
            let mut total = call.arg_or_empty(1)?;
            for (index, item) in input.iter().enumerate() {
                total = call.lambda(0, item, index, Some(total))?;
            }
            Ok(Some(total))
        }
        _ => subsetting(call),
    }
}

/// Subsetting (N1 §5.3) and combining (§5.4).
fn subsetting<'a>(call: &Call<'_, '_, 'a>) -> Result<Option<Collection<'a>>, EvalError> {
    let model = call.model();
    let input = &call.input;
    match call.name {
        "single" => {
            call.arity(0, 0, "no")?;
            match input.as_slice() {
                [] | [_] => Ok(Some(input.clone())),
                items => Err(EvalError::singleton("single", items.len())),
            }
        }
        "first" => {
            call.arity(0, 0, "no")?;
            Ok(Some(input.first().cloned().into_iter().collect()))
        }
        "last" => {
            call.arity(0, 0, "no")?;
            Ok(Some(input.last().cloned().into_iter().collect()))
        }
        "tail" => {
            call.arity(0, 0, "no")?;
            Ok(Some(input.iter().skip(1).cloned().collect()))
        }
        "skip" | "take" => {
            call.arity(1, 1, "1")?;
            let Some(count) = integer(model, &call.arg_or_empty(0)?, call.name)? else {
                return Ok(Some(Vec::new()));
            };
            let count = usize::try_from(count.max(0)).unwrap_or(0);
            Ok(Some(if call.name == "skip" {
                input.iter().skip(count).cloned().collect()
            } else {
                input.iter().take(count).cloned().collect()
            }))
        }
        "intersect" => {
            call.arity(1, 1, "1")?;
            let other = call.arg_or_empty(0)?;
            let mut out = Vec::new();
            for item in input {
                if contains(model, &other, item)? && !contains(model, &out, item)? {
                    out.push(item.clone());
                }
            }
            Ok(Some(out))
        }
        "exclude" => {
            call.arity(1, 1, "1")?;
            let other = call.arg_or_empty(0)?;
            let mut out = Vec::new();
            for item in input {
                if !contains(model, &other, item)? {
                    out.push(item.clone());
                }
            }
            Ok(Some(out))
        }
        "union" => {
            call.arity(1, 1, "1")?;
            let mut all = input.clone();
            all.extend(call.arg_or_empty(0)?);
            distinct(model, all).map(Some)
        }
        "combine" => {
            call.arity(1, 1, "1")?;
            let mut all = input.clone();
            all.extend(call.arg_or_empty(0)?);
            Ok(Some(all))
        }
        _ => Ok(None),
    }
}

/// The items whose criterion is true (N1 §5.2.1).
fn filter<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    let mut out = Vec::new();
    for (index, item) in call.input.iter().enumerate() {
        if boolean(call.model(), &call.lambda(0, item, index, None)?, call.name)? == Some(true) {
            out.push(item.clone());
        }
    }
    Ok(out)
}

/// The projection applied while it yields new items (N1 §5.2.3).
fn repeat<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    let model = call.model();
    let mut out: Collection<'a> = Vec::new();
    let mut pending: Collection<'a> = call.input.clone();
    while !pending.is_empty() {
        let mut next = Vec::new();
        for (index, item) in pending.iter().enumerate() {
            for found in call.lambda(0, item, index, None)? {
                if !contains(model, &out, &found)? {
                    out.push(found.clone());
                    next.push(found);
                }
            }
        }
        pending = next;
    }
    Ok(out)
}

/// The type specifier the argument of `is()`, `as()` or `ofType()` spells.
fn type_argument(call: &Call<'_, '_, '_>) -> Result<crate::ast::TypeSpecifier, EvalError> {
    call.args
        .first()
        .and_then(crate::ast::Expr::as_type_specifier)
        .ok_or_else(|| EvalError::Argument {
            function: String::from(call.name),
            reason: String::from("the argument is not a type specifier"),
        })
}
