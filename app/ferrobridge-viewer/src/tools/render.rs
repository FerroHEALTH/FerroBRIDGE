// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answers of the tools, drawn on the server.
//!
//! They are the `Bundle` a `$tofhir`
//! answered, the composition and the outcome a `$toopenehr` answered, and
//! the `OperationOutcome` a `$validate` answered, its issues ordered by
//! severity, each with its raw JSON behind a disclosure.

use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhir_types::r4::parameters::{Parameters, ParametersParameterValue};
use fhir_types::r4::resource::Resource;
use leptos::prelude::*;

use crate::bridge::BridgeError;
use crate::engine::render::parts;

/// The section of input the console does not send: why, by field.
#[must_use]
pub fn invalid(heading: &'static str, reason: &str) -> String {
    view! {
        <section class="card">
            <h2>{heading}</h2>
            <p role="alert">{reason.to_owned()}</p>
        </section>
    }
    .to_html()
}

/// The answer of `$tofhir`: each resource of the `Bundle`, and the whole
/// `Bundle` as JSON.
#[must_use]
pub fn bundle(answer: &Result<serde_json::Value, BridgeError>) -> String {
    let section = match answer {
        Ok(bundle) => {
            let entries = bundle
                .get("entry")
                .and_then(serde_json::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let items = entries
                .iter()
                .map(|entry| {
                    let resource = entry.get("resource").unwrap_or(&serde_json::Value::Null);
                    let kind = resource
                        .get("resourceType")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("an entry with no resource")
                        .to_owned();
                    let id = resource
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .map(|id| format!("/{id}"))
                        .unwrap_or_default();
                    // NOTE: no specification governs this: our own design; a value
                    // parsed from JSON always prints, so a failure leaves it empty.
                    let text = serde_json::to_string_pretty(resource).unwrap_or_default();
                    view! {
                        <li>
                            <h3>
                                <code>{format!("{kind}{id}")}</code>
                            </h3>
                            <pre class="code-block">{text}</pre>
                        </li>
                    }
                })
                .collect_view();
            let count = entries.len();
            view! {
                <section class="card">
                    <h2>"$tofhir answered"</h2>
                    <p>{format!("The Bundle holds {count} resources.")}</p>
                    <ol class="entries">{items}</ol>
                    {parts::raw_json("The Bundle as JSON", bundle)}
                </section>
            }
            .into_any()
        }
        Err(error) => parts::failed("$tofhir answered", error),
    };
    view! { <div class="sections">{section}</div> }.to_html()
}

/// The answer of `$toopenehr`: the composition, the outcome's issues by
/// severity, and the whole `Parameters` as JSON.
#[must_use]
pub fn composition(answer: &Result<(Parameters, serde_json::Value), BridgeError>) -> String {
    let section = match answer {
        Ok((parameters, raw)) => {
            let mut text = None;
            let mut outcome = None;
            for parameter in &parameters.parameter {
                match parameter.name.value.as_deref() {
                    Some("composition") => {
                        if let Some(ParametersParameterValue::String(value)) = &parameter.value {
                            text.clone_from(&value.value);
                        }
                    }
                    Some("outcome") => {
                        if let Some(Resource::OperationOutcome(carried)) = &parameter.resource {
                            outcome = Some(carried.as_ref().clone());
                        }
                    }
                    _ => {}
                }
            }
            let shown = match text {
                // NOTE: no specification governs this: our own design; a
                // composition that is no JSON is shown as the text it is.
                Some(text) => serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|value| serde_json::to_string_pretty(&value).ok())
                    .unwrap_or(text),
                None => String::from("The answer carries no composition."),
            };
            let issues = outcome.as_ref().map(issues_table);
            view! {
                <section class="card">
                    <h2>"$toopenehr answered"</h2>
                    <h3>"Composition"</h3>
                    <pre class="code-block">{shown}</pre>
                    {issues}
                    {parts::raw_json("The Parameters as JSON", raw)}
                </section>
            }
            .into_any()
        }
        Err(error) => parts::failed("$toopenehr answered", error),
    };
    view! { <div class="sections">{section}</div> }.to_html()
}

/// The answer of `$validate`: its issues by severity, and the
/// `OperationOutcome` as JSON.
#[must_use]
pub fn outcome(answer: &Result<(OperationOutcome, serde_json::Value), BridgeError>) -> String {
    let section = match answer {
        Ok((outcome, raw)) => view! {
            <section class="card">
                <h2>"$validate answered"</h2>
                {issues_table(outcome)}
                {parts::raw_json("The OperationOutcome as JSON", raw)}
            </section>
        }
        .into_any(),
        Err(error) => parts::failed("$validate answered", error),
    };
    view! { <div class="sections">{section}</div> }.to_html()
}

/// The rank of an issue severity: `fatal` and `error` first, `information`
/// last (FHIR R4 `valueset-issue-severity.html`).
fn rank(issue: &OperationOutcomeIssue) -> u8 {
    match issue.severity.value.as_deref() {
        Some("fatal") => 0,
        Some("error") => 1,
        Some("warning") => 2,
        Some("information") => 3,
        _ => 4,
    }
}

/// The issues of `outcome`, ordered by severity, each with its severity as a
/// word, its code and its diagnostics.
fn issues_table(outcome: &OperationOutcome) -> AnyView {
    let mut issues: Vec<&OperationOutcomeIssue> = outcome.issue.iter().collect();
    issues.sort_by_key(|issue| rank(issue));
    let rows = issues
        .into_iter()
        .map(|issue| {
            let severity = issue.severity.value.clone().unwrap_or_default();
            let tone = match severity.as_str() {
                "fatal" | "error" => "badge tone-danger",
                "warning" => "badge tone-warn",
                _ => "badge tone-neutral",
            };
            let code = issue.code.value.clone().unwrap_or_default();
            let diagnostics = issue
                .diagnostics
                .as_ref()
                .and_then(|text| text.value.clone())
                .unwrap_or_default();
            let place = issue
                .expression
                .iter()
                .filter_map(|expression| expression.value.clone())
                .collect::<Vec<_>>()
                .join(", ");
            view! {
                <tr>
                    <td>
                        <span class=tone>{severity}</span>
                    </td>
                    <td>
                        <code>{code}</code>
                    </td>
                    <td>
                        <code>{place}</code>
                    </td>
                    <td>{diagnostics}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <table>
            <caption>"Issues, by severity"</caption>
            <thead>
                <tr>
                    <th scope="col">"Severity"</th>
                    <th scope="col">"Code"</th>
                    <th scope="col">"Where"</th>
                    <th scope="col">"Diagnostics"</th>
                </tr>
            </thead>
            <tbody>{rows}</tbody>
        </table>
    }
    .into_any()
}
