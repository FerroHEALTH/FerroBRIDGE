// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every engine-state section shares: the reading of a field of the
//! bridge's JSON, the section a refusal or a fault draws in place of its
//! content, and the list of diagnostics.

use leptos::prelude::*;
use openehr_mapping_core::diagnostic::{Diagnostic, Severity};

use crate::bridge::BridgeError;
use crate::model::{Refusal, ViewError};

/// A field the bridge's answer lacks or carries in another shape, named by
/// its path in the document.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the bridge's answer carries no {path} of the shape this console reads")]
pub struct Shape {
    /// Where the field was looked for, such as `lanes` or `build.commit`.
    pub path: &'static str,
}

/// The value at `path` of `value`, a `.`-separated run of object keys.
///
/// # Errors
/// Returns [`Shape`] when a key is missing or a step is no object.
pub fn field<'a>(
    value: &'a serde_json::Value,
    path: &'static str,
) -> Result<&'a serde_json::Value, Shape> {
    path.split('.')
        .try_fold(value, |at, key| at.get(key))
        .ok_or(Shape { path })
}

/// The text at `path` of `value`.
///
/// # Errors
/// Returns [`Shape`] when it is missing or no string.
pub fn text<'a>(value: &'a serde_json::Value, path: &'static str) -> Result<&'a str, Shape> {
    field(value, path)?.as_str().ok_or(Shape { path })
}

/// The text at the optional `key` of `value`: `None` for a key that is
/// absent or `null`.
///
/// # Errors
/// Returns [`Shape`] when it is present and neither a string nor `null`.
pub fn optional_text<'a>(
    value: &'a serde_json::Value,
    key: &'static str,
) -> Result<Option<&'a str>, Shape> {
    match value.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(Shape { path: key }),
    }
}

/// The section a failed bridge call draws in place of its content: the
/// refusal with the bridge's status and diagnostics, or the console's fault,
/// never nothing.
#[must_use]
pub fn failed(heading: &'static str, error: &BridgeError) -> AnyView {
    let said = match crate::model::server::refusal(error) {
        Ok(refusal) => refused(&refusal),
        Err(fault) => faulted(&fault),
    };
    view! {
        <section class="card">
            <h2>{heading}</h2>
            {said}
        </section>
    }
    .into_any()
}

/// The section an answer this console cannot read draws.
#[must_use]
pub fn unreadable(heading: &'static str, shape: &Shape) -> AnyView {
    tracing::error!(%shape, "the bridge answered a document this console cannot read");
    view! {
        <section class="card">
            <h2>{heading}</h2>
            <p role="alert">{shape.to_string()}</p>
        </section>
    }
    .into_any()
}

/// The words of `refusal`.
fn refused(refusal: &Refusal) -> AnyView {
    match refusal {
        Refusal::NotAuthenticated => view! {
            <p role="alert">"401: the bridge did not accept your sign-in. Sign in again."</p>
        }
        .into_any(),
        Refusal::Bridge {
            status,
            diagnostics,
        } => {
            let items = diagnostics
                .iter()
                .map(|text| view! { <li>{text.clone()}</li> })
                .collect_view();
            view! {
                <p role="alert">{format!("{status}: the bridge refused the request.")}</p>
                <ul>{items}</ul>
            }
            .into_any()
        }
        Refusal::Invalid { reason } => view! { <p role="alert">{reason.clone()}</p> }.into_any(),
    }
}

/// The words of the console's fault `fault`.
fn faulted(fault: &ViewError) -> AnyView {
    view! { <p role="alert">{fault.to_string()}</p> }.into_any()
}

/// The word and tone class of `severity`.
#[must_use]
pub const fn severity(severity: Severity) -> (&'static str, &'static str) {
    match severity {
        Severity::Error => ("error", "badge tone-danger"),
        Severity::Warning => ("warning", "badge tone-warn"),
    }
}

/// The table of `diagnostics`, each with its severity as a word, its code,
/// where it is, its message and its mapping; a sentence when there is none.
#[must_use]
pub fn diagnostics_table(diagnostics: &[Diagnostic]) -> AnyView {
    if diagnostics.is_empty() {
        return view! { <p>"No diagnostics."</p> }.into_any();
    }
    let rows = diagnostics
        .iter()
        .map(|diagnostic| {
            let (word, tone) = severity(diagnostic.severity());
            let place = match diagnostic.position() {
                Some(position) => format!("{}:{position}", diagnostic.file().display()),
                None => diagnostic.file().display().to_string(),
            };
            let mapping = diagnostic
                .mapping_name()
                .map(|name| name.as_str().to_owned())
                .unwrap_or_default();
            view! {
                <tr>
                    <td>
                        <span class=tone>{word}</span>
                    </td>
                    <td>
                        <code>{diagnostic.code().as_str().to_owned()}</code>
                    </td>
                    <td>
                        <code>{place}</code>
                    </td>
                    <td>{diagnostic.message().to_owned()}</td>
                    <td>{mapping}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <div class="table-scroll" tabindex="0">
            <table>
                <caption>"Diagnostics"</caption>
                <thead>
                    <tr>
                        <th scope="col">"Severity"</th>
                        <th scope="col">"Code"</th>
                        <th scope="col">"Where"</th>
                        <th scope="col">"Message"</th>
                        <th scope="col">"Mapping"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
    .into_any()
}

/// A `<details>` holding `json` pretty-printed, under `summary`.
#[must_use]
pub fn raw_json(summary: &'static str, json: &serde_json::Value) -> AnyView {
    // NOTE: no specification governs this: our own design; a value parsed from
    // JSON always prints, so a failure here leaves the disclosure empty.
    let text = serde_json::to_string_pretty(json).unwrap_or_default();
    view! {
        <details class="disclosure">
            <summary>{summary}</summary>
            <pre>{text}</pre>
        </details>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::{Shape, field, optional_text, text};

    #[test]
    fn a_field_is_read_by_path_and_a_missing_one_is_named() {
        let value = serde_json::json!({"build": {"commit": "abc"}, "n": 1});
        assert_eq!(Ok("abc"), text(&value, "build.commit"));
        assert_eq!(
            Err(Shape {
                path: "build.rustc"
            }),
            field(&value, "build.rustc").map(|_| ())
        );
        assert_eq!(Err(Shape { path: "n" }), text(&value, "n"));
        assert_eq!(Ok(None), optional_text(&value, "absent"));
        assert_eq!(Err(Shape { path: "n" }), optional_text(&value, "n"));
    }
}
