// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every engine-state section shares.
//!
//! It is the reading of a field of the bridge's JSON, the section a refusal
//! or a fault draws in place of its content, the table and the list of
//! diagnostics, each drawn with the components of `ferrohealth_ui`.

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::table::view::{Body, Column, DataTable, data_table, data_table_unstacked};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::code::code_panel;
use ferrohealth_ui::view::refusal::fault;
use ferrohealth_ui::view::surface::{card, disclosure};
use leptos::prelude::*;
use openehr_mapping_core::diagnostic::{Diagnostic, Severity};

use crate::bridge::BridgeError;
use crate::locale::{Msg, tr, tr_with};
use crate::model::{Refusal, ViewError};

/// A field the bridge's answer lacks or carries in another shape, named by
/// its path in the document.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the bridge's answer carries no {path} of the shape this console reads")]
pub struct Shape {
    /// Where the field was looked for, such as `lanes` or `build.commit`.
    pub path: &'static str,
}

impl Shape {
    /// The words of the fault in the page's locale; its `Display` is the log's.
    #[must_use]
    pub fn text(&self) -> String {
        tr_with(Msg::ShapeMissing, &[("path", self.path.into())])
    }
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
pub fn failed(heading: &str, error: &BridgeError) -> AnyView {
    let said = match crate::model::server::refusal(error) {
        Ok(refusal) => refused(&refusal),
        Err(failure) => fault(failure.text()).into_any(),
    };
    card(heading.to_owned(), said).into_any()
}

/// The section an answer this console cannot read draws.
#[must_use]
pub fn unreadable(heading: &str, shape: &Shape) -> AnyView {
    tracing::error!(%shape, "the bridge answered a document this console cannot read");
    card(heading.to_owned(), fault(shape.text())).into_any()
}

/// The words of `refusal`: its status and the diagnostics it carried.
#[must_use]
pub fn refused(refusal: &Refusal) -> AnyView {
    match refusal {
        Refusal::NotAuthenticated => fault(tr(Msg::N401BridgeDidNotAcceptYour)).into_any(),
        Refusal::Bridge {
            status,
            diagnostics,
        } => {
            let alert = fault(tr_with(
                Msg::BridgeRefusedStatus,
                &[("status", status.to_string().into())],
            ));
            let items = diagnostics
                .iter()
                .map(|text| view! { <li>{text.clone()}</li> })
                .collect_view();
            let list =
                (!diagnostics.is_empty()).then(|| view! { <ul class="fui-issues">{items}</ul> });
            view! {
                {alert}
                {list}
            }
            .into_any()
        }
        Refusal::Invalid { reason } => fault(reason.clone()).into_any(),
    }
}

/// The console's own fault `failure`, as an alert.
#[must_use]
pub fn faulted(failure: &ViewError) -> AnyView {
    fault(failure.text()).into_any()
}

/// The word and the badge classes of `severity`.
#[must_use]
pub fn severity(severity: Severity) -> (String, &'static str) {
    match severity {
        Severity::Error => (tr(Msg::SeverityError), Tone::Bad.badge()),
        Severity::Warning => (tr(Msg::SeverityWarning), Tone::Warn.badge()),
    }
}

/// The library's data table under `caption`, headed `columns`, over `rows`.
///
/// The rows stack as cards on a phone: every body cell but the row's name
/// carries its column's `data-label`. The console's tables do not sort.
#[must_use]
pub fn table(caption: &str, columns: &[&str], rows: AnyView) -> AnyView {
    columns_table(caption, &text_columns(columns), rows)
}

/// The data table [`table`] draws, headed by `columns`.
#[must_use]
pub fn columns_table(caption: &str, columns: &[Column], rows: AnyView) -> AnyView {
    data_table(DataTable {
        caption,
        columns,
        sort: None,
        sort_href: &|_| String::new(),
        body: Body::Rows(rows),
    })
    .into_any()
}

/// The data table [`table`] draws, as a grid at every width: for a table of
/// two short columns, which fits a phone as it is.
#[must_use]
pub fn table_unstacked(caption: &str, columns: &[&str], rows: AnyView) -> AnyView {
    data_table_unstacked(DataTable {
        caption,
        columns: &text_columns(columns),
        sort: None,
        sort_href: &|_| String::new(),
        body: Body::Rows(rows),
    })
    .into_any()
}

/// A text column for each label of `labels`, keyed by its label.
#[must_use]
pub fn text_columns(labels: &[&str]) -> Vec<Column> {
    labels
        .iter()
        .copied()
        .map(|label| Column::owned(label, label))
        .collect()
}

/// The table of `diagnostics`, each with its severity as a word, its code,
/// where it is, its message and its mapping; a sentence when there is none.
#[must_use]
pub fn diagnostics_table(diagnostics: &[Diagnostic]) -> AnyView {
    linked_diagnostics_table(diagnostics, &|_| None)
}

/// The table of `diagnostics` as [`diagnostics_table`] draws it, where the
/// place of a diagnostic that `href` names a target for links to it.
#[must_use]
pub fn linked_diagnostics_table(
    diagnostics: &[Diagnostic],
    href: &dyn Fn(&Diagnostic) -> Option<String>,
) -> AnyView {
    if diagnostics.is_empty() {
        return view! { <p>{tr(Msg::NoDiagnostics)}</p> }.into_any();
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
            let place = match href(diagnostic) {
                Some(target) => view! {
                    <a href=target>
                        <code>{place}</code>
                    </a>
                }
                .into_any(),
                None => view! { <code>{place}</code> }.into_any(),
            };
            view! {
                <tr>
                    <td>
                        <span class=tone>{word}</span>
                    </td>
                    <td data-label=tr(Msg::Code)>
                        <code>{diagnostic.code().as_str().to_owned()}</code>
                    </td>
                    <td data-label=tr(Msg::Where)>{place}</td>
                    <td data-label=tr(Msg::Message)>{diagnostic.message().to_owned()}</td>
                    <td data-label=tr(Msg::Mapping)>{mapping}</td>
                </tr>
            }
        })
        .collect_view()
        .into_any();
    table(
        &tr(Msg::Diagnostics),
        &[
            &tr(Msg::Severity),
            &tr(Msg::Code),
            &tr(Msg::Where),
            &tr(Msg::Message),
            &tr(Msg::Mapping),
        ],
        rows,
    )
}

/// A disclosure holding `json` pretty-printed and highlighted, under
/// `summary`, in a bounded panel.
///
/// # Panics
///
/// Panics if `serde_json` cannot write a `serde_json::Value`, which it
/// always can: its map keys are strings and its `Serialize` never fails
/// (<https://docs.rs/serde_json/1/serde_json/fn.to_string_pretty.html>, Errors).
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "to_string_pretty fails only for a Serialize that fails or a map with non-string keys, and a serde_json::Value has neither"
)]
pub fn raw_json(summary: &str, json: &serde_json::Value) -> AnyView {
    let text =
        serde_json::to_string_pretty(json).expect("a serde_json::Value should always serialize");
    disclosure(
        summary.to_owned(),
        code_panel(summary, Language::Json, &text),
    )
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::{Shape, field, optional_text, raw_json, text};

    #[test]
    fn raw_json_draws_every_member_of_the_value_pretty_printed() {
        let value = serde_json::json!({
            "resourceType": "Parameters",
            "parameter": [{"name": "réponse", "valueInteger": 7}],
        });
        let html = leptos::prelude::Owner::new()
            .with(|| leptos::prelude::RenderHtml::to_html(raw_json("The Parameters", &value)));
        assert!(html.contains("The Parameters"), "{html}");
        assert!(html.contains("resourceType"), "{html}");
        assert!(html.contains("réponse"), "{html}");
        assert!(html.contains("valueInteger"), "{html}");
    }

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
