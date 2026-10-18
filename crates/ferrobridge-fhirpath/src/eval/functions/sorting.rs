// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `sort()`, which FHIRPath 3.0.0 adds as Standard for Trial Use
//! (<http://hl7.org/fhirpath/STU3/#sortkeyselector-this--any-asc--desc--keyselector-this--any-asc--desc---collection>).
//! It is reached only with `Environment::with_fhirpath_stu3_functions`.

use std::cmp::Ordering;

use super::Call;
use crate::ast::Expr;
use crate::eval::Frame;
use crate::eval::convert::single_value;
use crate::eval::error::EvalError;
use crate::eval::operators::compare;
use crate::value::{Collection, Item};

/// One key of each item: its value, `None` when the key selector answers
/// empty, which sorts before every value.
type Key<'a> = Option<Item<'a>>;

/// Calls `sort()` when the call names it.
pub(super) fn call<'a>(call: &Call<'_, '_, 'a>) -> Result<Option<Collection<'a>>, EvalError> {
    if call.name != "sort" {
        return Ok(None);
    }
    let model = call.model();
    let mut keyed = Vec::with_capacity(call.input.len());
    for item in &call.input {
        let keys = if call.args.is_empty() {
            vec![single_value(model, &vec![item.clone()], call.name)?]
        } else {
            let mut keys = Vec::with_capacity(call.args.len());
            for arg in call.args {
                let selector = match arg {
                    Expr::SortKey { key, .. } => key.as_ref(),
                    other => other,
                };
                // NOTE: FHIRPath 3.0.0 sort(): $index is undefined in a key selector, so the
                // selector sees the item as $this alone.
                let frame = Frame {
                    focus: vec![item.clone()],
                    index: None,
                    total: None,
                };
                let selected = call.engine.eval(selector, &frame)?;
                keys.push(single_value(model, &selected, call.name)?);
            }
            keys
        };
        keyed.push((keys, item.clone()));
    }
    let descending: Vec<bool> = call
        .args
        .iter()
        .map(|arg| {
            matches!(
                arg,
                Expr::SortKey {
                    descending: true,
                    ..
                }
            )
        })
        .collect();
    let mut failure = None;
    keyed.sort_by(|(left, _), (right, _)| {
        if failure.is_some() {
            return Ordering::Equal;
        }
        match order(left, right, &descending) {
            Ok(order) => order,
            Err(error) => {
                failure = Some(error);
                Ordering::Equal
            }
        }
    });
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(Some(keyed.into_iter().map(|(_, item)| item).collect()))
}

/// The order of two items by their keys, the first key that tells them apart
/// deciding, descending where `descending` says so.
fn order(left: &[Key<'_>], right: &[Key<'_>], descending: &[bool]) -> Result<Ordering, EvalError> {
    for (index, (a, b)) in left.iter().zip(right).enumerate() {
        let order = match (a, b) {
            (None, None) => Ordering::Equal,
            // NOTE: FHIRPath 3.0.0 sort(): an empty value is lower than all other values.
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            // NOTE: FHIRPath 3.0.0 sort() is silent on values whose comparison is unknown, as
            // across date precisions; no specification governs this: our own design keeps them.
            (Some(a), Some(b)) => compare(a, b, "sort")?.unwrap_or(Ordering::Equal),
        };
        let order = if descending.get(index).copied().unwrap_or(false) {
            order.reverse()
        } else {
            order
        };
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(Ordering::Equal)
}
