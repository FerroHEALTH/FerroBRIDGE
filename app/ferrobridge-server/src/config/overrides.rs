// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The environment overrides, applied over the file before the tree is read.
//!
//! An environment variable carries text, and the tree decides what the text
//! is. Every override is first placed as the string it is, so a string key
//! takes `12345`, `true` or `1.5` verbatim. When the tree then refuses a
//! string where it wants a number, a boolean or an array, and the text reads
//! as that TOML value, the override is placed typed and the tree is read
//! again. No specification governs this: our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crate::telemetry::{FILTER_ENV, FORMAT_ENV};

use super::ENV_PREFIX;
use super::Error;

/// One `FERROBRIDGE__` variable, with the key it addresses.
#[derive(Debug)]
pub(super) struct Override {
    /// The variable name, for a refusal.
    name: String,
    /// The lower-case key segments, the section first.
    path: Vec<String>,
    /// The text the variable carries.
    raw: String,
    /// The text read as a TOML value that is not a string, when it reads so.
    typed: Option<toml::Value>,
}

/// Returns the override `name` makes, or `None` for a variable outside the
/// prefix.
///
/// # Errors
/// Returns [`Error::EnvName`] for a name that addresses no section and key.
pub(super) fn parse_override(name: &str, raw: &str) -> Result<Option<Override>, Error> {
    let Some(rest) = name.strip_prefix(ENV_PREFIX) else {
        return Ok(None);
    };
    let path: Vec<String> = rest.split("__").map(str::to_ascii_lowercase).collect();
    if path.len() < 2 || path.iter().any(String::is_empty) {
        return Err(Error::EnvName {
            name: name.to_owned(),
        });
    }
    Ok(Some(Override {
        name: name.to_owned(),
        path,
        raw: raw.to_owned(),
        typed: typed_value(raw),
    }))
}

/// Applies `item` onto `table`, typed or as its text.
///
/// # Errors
/// Returns [`Error::EnvShape`] when a parent segment holds a value that is not
/// a section.
pub(super) fn apply_override(
    table: &mut toml::Table,
    item: &Override,
    typed: bool,
) -> Result<(), Error> {
    let Some((key, parents)) = item.path.split_last() else {
        return Err(Error::EnvName {
            name: item.name.clone(),
        });
    };
    let mut cursor = table;
    for parent in parents {
        let entry = cursor
            .entry(parent.clone())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let toml::Value::Table(next) = entry else {
            return Err(Error::EnvShape {
                name: item.name.clone(),
            });
        };
        cursor = next;
    }
    let value = match (&item.typed, typed) {
        (Some(value), true) => value.clone(),
        _ => toml::Value::String(item.raw.clone()),
    };
    cursor.insert(key.clone(), value);
    Ok(())
}

/// Returns the index of the override the tree refused as a string and that
/// reads as another TOML value, so it is placed typed on the next read.
///
/// `merged` is the text the tree was read from, `refused` the span its
/// refusal names, and `typed` the overrides already placed typed.
pub(super) fn retype(
    overrides: &[Override],
    typed: &BTreeSet<usize>,
    merged: &str,
    refused: &Range<usize>,
) -> Option<usize> {
    // NOTE: no specification governs this: our own design. A merged text that
    // does not parse leaves the refusal standing as the parse reported it.
    let document = toml::de::DeTable::parse(merged).ok()?.into_inner();
    overrides.iter().enumerate().find_map(|(index, item)| {
        if typed.contains(&index) || item.typed.is_none() {
            return None;
        }
        let span = value_span(&document, &item.path)?;
        (span.start <= refused.start && refused.end <= span.end).then_some(index)
    })
}

/// Returns the span of the value `path` addresses in `document`.
fn value_span(document: &toml::de::DeTable<'_>, path: &[String]) -> Option<Range<usize>> {
    let (first, rest) = path.split_first()?;
    let mut value = document.get(first.as_str())?;
    for segment in rest {
        value = value.get_ref().get(segment.as_str())?;
    }
    Some(value.span())
}

/// Applies [`FORMAT_ENV`] and [`FILTER_ENV`] onto `[telemetry]`.
///
/// They run after every `FERROBRIDGE__` override, so they win over the file
/// and over `FERROBRIDGE__TELEMETRY__FORMAT` and `FERROBRIDGE__TELEMETRY__FILTER`:
/// they are the names an operator sets for one run. Each value is text, so a
/// format outside `auto`, `json` and `pretty` is refused by the parse that
/// follows, naming `telemetry.format`.
pub(super) fn apply_console_overrides(
    table: &mut toml::Table,
    environment: &BTreeMap<String, String>,
) -> Result<(), Error> {
    for (variable, key) in [(FORMAT_ENV, "format"), (FILTER_ENV, "filter")] {
        let Some(raw) = environment.get(variable) else {
            continue;
        };
        let entry = table
            .entry("telemetry")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let toml::Value::Table(telemetry) = entry else {
            return Err(Error::EnvShape {
                name: variable.to_owned(),
            });
        };
        telemetry.insert(key.to_owned(), toml::Value::String(raw.clone()));
    }
    Ok(())
}

/// Reads `raw` as a TOML value that is not a string: a number, a boolean, a
/// date-time, an array or an inline table.
fn typed_value(raw: &str) -> Option<toml::Value> {
    // NOTE: no specification governs this: our own design. A parse failure IS
    // the answer here, because text that is not TOML syntax stays a string.
    let parsed = toml::from_str::<toml::Table>(&format!("value = {raw}"))
        .ok()
        .and_then(|table| table.get("value").cloned())?;
    (!parsed.is_str()).then_some(parsed)
}

#[cfg(test)]
mod tests {
    use super::{parse_override, typed_value};
    use crate::config::Error;

    #[test]
    fn an_environment_value_reads_typed_only_as_toml_syntax_that_is_not_a_string() {
        assert_eq!(Some(toml::Value::Integer(5)), typed_value("5"));
        assert_eq!(Some(toml::Value::Boolean(true)), typed_value("true"));
        assert_eq!(Some(toml::Value::Float(1.5)), typed_value("1.5"));
        assert_eq!(None, typed_value("\"quoted\""));
        assert_eq!(None, typed_value("http://cdr.invalid/v1"));
        assert_eq!(None, typed_value("0.0.0.0:8080"));
    }

    #[test]
    fn an_override_name_without_a_section_and_a_key_is_refused() {
        let error = parse_override("FERROBRIDGE__LISTEN", "x")
            .expect_err("a section and a key are both required");
        assert!(matches!(error, Error::EnvName { .. }), "{error:?}");
    }
}
