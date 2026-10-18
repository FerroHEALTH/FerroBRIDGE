// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The string functions FHIRPath 3.0.0 adds as Standard for Trial Use:
//! `matchesFull()` (<http://hl7.org/fhirpath/STU3/#matchesfullregex--string-flags--string--boolean>)
//! and the additional string functions
//! (<http://hl7.org/fhirpath/STU3/#additional-string-functions>). They are
//! reached only with `Environment::with_fhirpath_stu3_functions`.

use std::borrow::Cow;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE};
use regex::RegexBuilder;

use super::Call;
use crate::eval::convert::string;
use crate::eval::error::EvalError;
use crate::value::{Collection, Item};

/// The names this module defines, with their admitted argument counts.
const FUNCTIONS: [(&str, usize, usize, &str); 8] = [
    ("matchesFull", 1, 2, "1 or 2"),
    ("encode", 1, 1, "1"),
    ("decode", 1, 1, "1"),
    ("escape", 1, 1, "1"),
    ("unescape", 1, 1, "1"),
    ("trim", 0, 0, "no"),
    ("split", 1, 1, "1"),
    ("join", 0, 1, "0 or 1"),
];

/// Calls the function when this module defines it.
pub(super) fn call<'a>(call: &Call<'_, '_, 'a>) -> Result<Option<Collection<'a>>, EvalError> {
    let Some((_, min, max, expected)) = FUNCTIONS.iter().find(|(name, ..)| *name == call.name)
    else {
        return Ok(None);
    };
    call.arity(*min, *max, expected)?;
    if call.name == "join" {
        return join(call).map(Some);
    }
    let model = call.model();
    let Some(text) = string(model, &call.input, call.name)? else {
        return Ok(Some(Vec::new()));
    };
    let text_arg = |index: usize| -> Result<Option<String>, EvalError> {
        string(model, &call.arg_or_empty(index)?, call.name)
    };
    let strings = |values: Option<String>| Ok(Some(values.map(Item::String).into_iter().collect()));
    match call.name {
        "matchesFull" => {
            let Some(pattern) = text_arg(0)? else {
                return Ok(Some(Vec::new()));
            };
            let flags = text_arg(1)?.unwrap_or_default();
            Ok(Some(vec![Item::Boolean(matches_full(
                &text, &pattern, &flags,
            )?)]))
        }
        "encode" => match text_arg(0)? {
            Some(format) => strings(Some(encode(&text, &format)?)),
            None => strings(None),
        },
        "decode" => match text_arg(0)? {
            Some(format) => strings(decode(&text, &format)?),
            None => strings(None),
        },
        "escape" => match text_arg(0)? {
            Some(target) => strings(Some(escape(&text, &target)?)),
            None => strings(None),
        },
        "unescape" => match text_arg(0)? {
            Some(target) => strings(Some(unescape(&text, &target)?)),
            None => strings(None),
        },
        "trim" => strings(Some(String::from(text.trim_matches(is_whitespace)))),
        _ => split(call, &text),
    }
}

/// Whether `c` is whitespace in FHIRPath's lexical sense: tab, space, line
/// feed and carriage return (<http://hl7.org/fhirpath/STU3/#whitespace>), the
/// set `trim()` removes.
const fn is_whitespace(c: char) -> bool {
    matches!(c, '\t' | ' ' | '\n' | '\r')
}

/// An argument error of `function`.
fn argument(function: &str, reason: impl Into<String>) -> EvalError {
    EvalError::Argument {
        function: String::from(function),
        reason: reason.into(),
    }
}

/// Whether `text` matches `pattern` whole, the pattern surrounded by `^` and
/// `$`, in the single-line, case-sensitive mode `i` and `m` adjust.
fn matches_full(text: &str, pattern: &str, flags: &str) -> Result<bool, EvalError> {
    let mut builder = RegexBuilder::new(&format!("^(?:{pattern})$"));
    builder.dot_matches_new_line(true);
    for flag in flags.chars() {
        match flag {
            'i' => builder.case_insensitive(true),
            'm' => builder.multi_line(true),
            other => return Err(argument("matchesFull", format!("no flag {other:?}"))),
        };
    }
    let regex = builder.build().map_err(|source| EvalError::Regex {
        pattern: String::from(pattern),
        source,
    })?;
    Ok(regex.is_match(text))
}

/// `text` encoded in `format`: `hex`, `base64`, `urlbase64` or `ascii`.
fn encode(text: &str, format: &str) -> Result<String, EvalError> {
    Ok(match format {
        "hex" => text.bytes().fold(String::new(), |mut out, byte| {
            out.extend(
                [byte >> 4, byte & 0x0f]
                    .into_iter()
                    .filter_map(|nibble| char::from_digit(u32::from(nibble), 16)),
            );
            out
        }),
        "base64" => STANDARD.encode(text),
        "urlbase64" => URL_SAFE.encode(text),
        "ascii" => text
            .chars()
            .map(|c| if c.is_ascii() { c } else { '?' })
            .collect(),
        other => return Err(argument("encode", format!("no format {other:?}"))),
    })
}

/// `text` decoded from `format`, `None` when the bytes are no UTF-8.
fn decode(text: &str, format: &str) -> Result<Option<String>, EvalError> {
    let bytes = match format {
        "hex" => hex(text).ok_or_else(|| argument("decode", "the input is not hexadecimal"))?,
        "base64" => STANDARD
            .decode(text)
            .map_err(|error| argument("decode", format!("the input is not base64: {error}")))?,
        "urlbase64" => URL_SAFE
            .decode(text)
            .map_err(|error| argument("decode", format!("the input is not url base64: {error}")))?,
        other => return Err(argument("decode", format!("no format {other:?}"))),
    };
    // NOTE: FHIRPath 3.0.0 decode(): content that is not valid UTF-8 decodes to empty,
    // never to a replacement character.
    Ok(String::from_utf8(bytes).ok())
}

/// The bytes a string of hexadecimal digit pairs spells.
fn hex(text: &str) -> Option<Vec<u8>> {
    let digits = text.as_bytes();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    digits
        .chunks(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        })
        .collect()
}

/// `text` escaped for `target`: `html` or `json`.
fn escape(text: &str, target: &str) -> Result<String, EvalError> {
    match target {
        "html" => Ok(quick_xml::escape::escape(text).into_owned()),
        "json" => {
            let quoted = serde_json::to_string(text)
                .map_err(|error| argument("escape", format!("no JSON string: {error}")))?;
            Ok(quoted
                .strip_prefix('"')
                .and_then(|rest| rest.strip_suffix('"'))
                .map_or_else(|| quoted.clone(), String::from))
        }
        other => Err(argument("escape", format!("no target {other:?}"))),
    }
}

/// `text` unescaped from `target`: `html` or `json`.
fn unescape(text: &str, target: &str) -> Result<String, EvalError> {
    match target {
        "html" => quick_xml::escape::unescape(text)
            .map(Cow::into_owned)
            .map_err(|error| argument("unescape", format!("not escaped HTML: {error}"))),
        "json" => unescape_json(text),
        other => Err(argument("unescape", format!("no target {other:?}"))),
    }
}

/// `text` with the escapes of a JSON string (RFC 8259 §7) replaced by the
/// characters they stand for; a character that needs no escape stays as it
/// is, so a bare `"` comes back unchanged.
fn unescape_json(text: &str) -> Result<String, EvalError> {
    let bad = |what: &str| argument("unescape", format!("not an escaped JSON string: {what}"));
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    let code_unit = |chars: &mut std::str::Chars<'_>| -> Result<u16, EvalError> {
        let digits: String = chars.by_ref().take(4).collect();
        if digits.len() != 4 {
            return Err(bad("a \\u escape needs four hexadecimal digits"));
        }
        u16::from_str_radix(&digits, 16)
            .map_err(|error| bad(&format!("a \\u escape needs hexadecimal digits: {error}")))
    };
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let decoded = match chars.next() {
            Some('"') => '"',
            Some('\\') => '\\',
            Some('/') => '/',
            Some('b') => '\u{8}',
            Some('f') => '\u{c}',
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some('u') => {
                let first = code_unit(&mut chars)?;
                let units = if (0xD800..0xDC00).contains(&first) {
                    if chars.next() != Some('\\') || chars.next() != Some('u') {
                        return Err(bad("a high surrogate without its low surrogate"));
                    }
                    vec![first, code_unit(&mut chars)?]
                } else {
                    vec![first]
                };
                char::decode_utf16(units)
                    .next()
                    .and_then(Result::ok)
                    .ok_or_else(|| bad("an unpaired surrogate"))?
            }
            Some(other) => return Err(bad(&format!("no escape \\{other}"))),
            None => return Err(bad("a trailing backslash")),
        };
        out.push(decoded);
    }
    Ok(out)
}

/// `split()`: the parts of `text` between the separators.
fn split<'a>(call: &Call<'_, '_, 'a>, text: &str) -> Result<Option<Collection<'a>>, EvalError> {
    let Some(separator) = string(call.model(), &call.arg_or_empty(0)?, call.name)? else {
        return Ok(Some(Vec::new()));
    };
    // NOTE: FHIRPath 3.0.0 split() is silent on an empty separator; no specification
    // governs this: our own design, refused rather than splitting between characters.
    if separator.is_empty() {
        return Err(argument("split", "an empty separator"));
    }
    Ok(Some(
        text.split(separator.as_str())
            .map(|part| Item::String(String::from(part)))
            .collect(),
    ))
}

/// `join()`: the input strings in order, with the separator between them.
fn join<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    if call.input.is_empty() {
        return Ok(Vec::new());
    }
    let model = call.model();
    let separator = string(model, &call.arg_or_empty(0)?, call.name)?.unwrap_or_default();
    let mut parts = Vec::with_capacity(call.input.len());
    for item in &call.input {
        match string(model, &vec![item.clone()], call.name)? {
            Some(part) => parts.push(part),
            None => return Err(EvalError::type_error(call.name, "an empty item")),
        }
    }
    Ok(vec![Item::String(parts.join(&separator))])
}
