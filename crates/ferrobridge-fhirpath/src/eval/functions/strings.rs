// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! String manipulation (N1 §5.6). Positions and lengths count characters,
//! never bytes.

use regex::{Regex, RegexBuilder};

use super::Call;
use crate::eval::convert::{integer, string};
use crate::eval::error::EvalError;
use crate::value::{Collection, Item};

/// The names this module defines, with their admitted argument counts.
const FUNCTIONS: [(&str, usize, usize, &str); 12] = [
    ("indexOf", 1, 1, "1"),
    ("substring", 1, 2, "1 or 2"),
    ("startsWith", 1, 1, "1"),
    ("endsWith", 1, 1, "1"),
    ("contains", 1, 1, "1"),
    ("upper", 0, 0, "no"),
    ("lower", 0, 0, "no"),
    ("replace", 2, 2, "2"),
    ("matches", 1, 1, "1"),
    ("replaceMatches", 2, 2, "2"),
    ("length", 0, 0, "no"),
    ("toChars", 0, 0, "no"),
];

/// Compiles a pattern in the single-line, case-sensitive mode N1 §5.6.9
/// asks for.
fn compile(pattern: &str) -> Result<Regex, EvalError> {
    RegexBuilder::new(pattern)
        .dot_matches_new_line(true)
        .build()
        .map_err(|source| EvalError::Regex {
            pattern: String::from(pattern),
            source,
        })
}

/// Calls the function when this module defines it.
pub(super) fn call<'a>(call: &Call<'_, '_, 'a>) -> Result<Option<Collection<'a>>, EvalError> {
    let Some((_, min, max, expected)) = FUNCTIONS.iter().find(|(name, ..)| *name == call.name)
    else {
        return Ok(None);
    };
    call.arity(*min, *max, expected)?;
    let model = call.model();
    let Some(text) = string(model, &call.input, call.name)? else {
        return Ok(Some(Vec::new()));
    };
    let text_arg = |index: usize| -> Result<Option<String>, EvalError> {
        string(model, &call.arg_or_empty(index)?, call.name)
    };
    let some = |item: Item<'a>| Ok(Some(vec![item]));
    let none = || Ok(Some(Vec::new()));
    match call.name {
        "indexOf" => {
            let Some(part) = text_arg(0)? else {
                return none();
            };
            let index = text.find(&part).map_or(-1, |byte| {
                super::length(text.get(..byte).map_or(0, |head| head.chars().count()))
            });
            some(Item::Integer(index))
        }
        "substring" => {
            let Some(start) = integer(model, &call.arg_or_empty(0)?, call.name)? else {
                return none();
            };
            let length = integer(model, &call.arg_or_empty(1)?, call.name)?;
            let count = text.chars().count();
            let Ok(start) = usize::try_from(start) else {
                return none();
            };
            if start >= count {
                return none();
            }
            let taken: String = match length {
                Some(length) => text
                    .chars()
                    .skip(start)
                    .take(usize::try_from(length.max(0)).unwrap_or(0))
                    .collect(),
                None => text.chars().skip(start).collect(),
            };
            some(Item::String(taken))
        }
        "startsWith" | "endsWith" | "contains" => {
            let Some(part) = text_arg(0)? else {
                return none();
            };
            some(Item::Boolean(match call.name {
                "startsWith" => text.starts_with(&part),
                "endsWith" => text.ends_with(&part),
                _ => text.contains(&part),
            }))
        }
        "upper" => some(Item::String(text.to_uppercase())),
        "lower" => some(Item::String(text.to_lowercase())),
        "replace" => {
            let (Some(pattern), Some(substitution)) = (text_arg(0)?, text_arg(1)?) else {
                return none();
            };
            some(Item::String(text.replace(&pattern, &substitution)))
        }
        "matches" => {
            let Some(pattern) = text_arg(0)? else {
                return none();
            };
            some(Item::Boolean(compile(&pattern)?.is_match(&text)))
        }
        "replaceMatches" => {
            let (Some(pattern), Some(substitution)) = (text_arg(0)?, text_arg(1)?) else {
                return none();
            };
            // NOTE: N1 §5.6.10 is silent on an empty pattern; no specification governs
            // this: our own design, the input comes back unchanged as the HL7 test suite expects.
            if pattern.is_empty() {
                return some(Item::String(text));
            }
            some(Item::String(
                compile(&pattern)?
                    .replace_all(&text, substitution.as_str())
                    .into_owned(),
            ))
        }
        "length" => some(Item::Integer(super::length(text.chars().count()))),
        _ => Ok(Some(
            text.chars().map(|c| Item::String(c.to_string())).collect(),
        )),
    }
}
