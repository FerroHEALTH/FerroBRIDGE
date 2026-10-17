// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dashboard, drawn on the server.
//!
//! It shows the bridge's build facts and pins (`GET /health/info`), its lanes and listeners, its readiness indicators
//! (`GET /health/readiness`), and the counts of what its running load
//! compiled (`GET /operator/state`).

use std::collections::BTreeSet;

use http::StatusCode;
use leptos::prelude::*;
use openehr_mapping_core::diagnostic::Severity;

use crate::bridge::BridgeError;
use crate::engine::render::parts::{self, Shape};

/// The dashboard's sections for the three answers.
#[must_use]
pub fn render(
    info: &Result<serde_json::Value, BridgeError>,
    readiness: &Result<(StatusCode, serde_json::Value), BridgeError>,
    state: &Result<serde_json::Value, BridgeError>,
) -> String {
    let info_sections = match info {
        Ok(info) => facts(info).unwrap_or_else(|shape| parts::unreadable("Bridge", &shape)),
        Err(error) => parts::failed("Bridge", error),
    };
    let ready = match readiness {
        Ok((status, report)) => indicators(*status, report)
            .unwrap_or_else(|shape| parts::unreadable("Readiness", &shape)),
        Err(error) => parts::failed("Readiness", error),
    };
    let loaded = match state {
        Ok(state) => counts(state).unwrap_or_else(|shape| parts::unreadable("Mappings", &shape)),
        Err(error) => parts::failed("Mappings", error),
    };
    view! { <div class="sections">{info_sections} {ready} {loaded}</div> }.to_html()
}

/// The build facts, the lanes and the pins of `info`.
fn facts(info: &serde_json::Value) -> Result<AnyView, Shape> {
    let product = parts::text(info, "product")?.to_owned();
    let version = parts::text(info, "version")?.to_owned();
    let commit = parts::text(info, "build.commit_short")?.to_owned();
    let built = parts::text(info, "build.built_at")?.to_owned();
    let rustc = parts::text(info, "build.rustc")?.to_owned();
    let lanes = parts::array(info, "lanes")?
        .iter()
        .map(lane)
        .collect::<Result<Vec<_>, Shape>>()?;
    let pins = parts::array(info, "pins")?
        .iter()
        .map(|pin| {
            let name = parts::text(pin, "name")?.to_owned();
            let version = parts::text(pin, "version")?.to_owned();
            Ok(view! {
                <tr>
                    <th scope="row">{name}</th>
                    <td>{version}</td>
                </tr>
            })
        })
        .collect::<Result<Vec<_>, Shape>>()?;
    Ok(view! {
        <section class="card">
            <h2>"Bridge"</h2>
            <dl>
                <dt>"Product"</dt>
                <dd>{product}</dd>
                <dt>"Version"</dt>
                <dd>{version}</dd>
                <dt>"Commit"</dt>
                <dd>
                    <code>{commit}</code>
                </dd>
                <dt>"Built"</dt>
                <dd>{built}</dd>
                <dt>"Compiler"</dt>
                <dd>{rustc}</dd>
            </dl>
        </section>
        <section class="card">
            <h2>"Lanes and listeners"</h2>
            <table>
                <thead>
                    <tr>
                        <th scope="col">"Lane"</th>
                        <th scope="col">"Configured"</th>
                        <th scope="col">"Listener"</th>
                        <th scope="col">"Listening"</th>
                    </tr>
                </thead>
                <tbody>{lanes}</tbody>
            </table>
        </section>
        <section class="card">
            <h2>"Pins"</h2>
            <table>
                <thead>
                    <tr>
                        <th scope="col">"Specification or crate"</th>
                        <th scope="col">"Version"</th>
                    </tr>
                </thead>
                <tbody>{pins}</tbody>
            </table>
        </section>
    }
    .into_any())
}

/// One row of the lanes table.
fn lane(lane: &serde_json::Value) -> Result<AnyView, Shape> {
    let name = parts::text(lane, "name")?.to_owned();
    let enabled = parts::field(lane, "enabled")?.as_bool().ok_or(Shape {
        path: "lanes.enabled",
    })?;
    let listen = parts::optional_text(lane, "listen")?
        .map_or_else(|| String::from("none of its own"), str::to_owned);
    let up = match lane.get("up") {
        None | Some(serde_json::Value::Null) => ("not applicable", "badge tone-neutral"),
        Some(serde_json::Value::Bool(true)) => ("up", "badge tone-ok"),
        Some(serde_json::Value::Bool(false)) => ("down", "badge tone-danger"),
        Some(_) => return Err(Shape { path: "lanes.up" }),
    };
    let (configured, tone) = if enabled {
        ("on", "badge tone-ok")
    } else {
        ("off", "badge tone-neutral")
    };
    Ok(view! {
        <tr>
            <th scope="row">{name}</th>
            <td>
                <span class=tone>{configured}</span>
            </td>
            <td>
                <code>{listen}</code>
            </td>
            <td>
                <span class=up.1>{up.0}</span>
            </td>
        </tr>
    }
    .into_any())
}

/// The readiness report the bridge answered with `status`.
fn indicators(status: StatusCode, report: &serde_json::Value) -> Result<AnyView, Shape> {
    let state = parts::text(report, "state")?;
    let (word, tone) = match (state, status) {
        ("up", StatusCode::OK) => ("Ready", "pill tone-ok"),
        ("down", StatusCode::SERVICE_UNAVAILABLE) => ("Not ready", "pill tone-danger"),
        _ => return Err(Shape { path: "state" }),
    };
    let entries = parts::field(report, "indicators")?
        .as_object()
        .ok_or(Shape { path: "indicators" })?;
    let rows = entries
        .iter()
        .map(|(name, outcome)| {
            let up = parts::text(outcome, "state")? == "up";
            let detail = parts::optional_text(outcome, "detail")?
                .map(str::to_owned)
                .unwrap_or_default();
            let (word, tone) = if up {
                ("up", "badge tone-ok")
            } else {
                ("down", "badge tone-danger")
            };
            Ok(view! {
                <tr>
                    <th scope="row">{name.clone()}</th>
                    <td>
                        <span class=tone>{word}</span>
                    </td>
                    <td>{detail}</td>
                </tr>
            })
        })
        .collect::<Result<Vec<_>, Shape>>()?;
    let table = if rows.is_empty() {
        view! { <p>"The bridge registers no readiness indicator."</p> }.into_any()
    } else {
        view! {
            <table>
                <thead>
                    <tr>
                        <th scope="col">"Indicator"</th>
                        <th scope="col">"State"</th>
                        <th scope="col">"Detail"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        }
        .into_any()
    };
    Ok(view! {
        <section class="card">
            <h2>"Readiness"</h2>
            <p>
                <span class=tone>{word}</span>
            </p>
            {table}
        </section>
    }
    .into_any())
}

/// The counts of what the running load compiled, from `state`.
fn counts(state: &serde_json::Value) -> Result<AnyView, Shape> {
    let number = |path: &'static str| parts::field(state, path)?.as_u64().ok_or(Shape { path });
    let contexts = number("counts.contexts")?;
    let programs = number("counts.programs")?;
    let templates = number("counts.templates")?;
    let fhirconnect_files = number("counts.fhirconnect_files")?;
    let omocl_files = number("counts.omocl_files")?;
    let mut models = BTreeSet::new();
    for context in parts::array(state, "contexts")? {
        for model in parts::array(context, "models")? {
            models.insert(parts::text(model, "name")?.to_owned());
        }
    }
    let diagnostics = parts::diagnostics(state, "diagnostics")?;
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .count();
    let warnings = diagnostics.len().saturating_sub(errors);
    let models = models.len();
    Ok(view! {
        <section class="card">
            <h2>"Mappings"</h2>
            <dl class="counts">
                <dt>"Contexts loaded"</dt>
                <dd>{format!("{programs} of {contexts}")}</dd>
                <dt>"Models"</dt>
                <dd>{models.to_string()}</dd>
                <dt>"Templates"</dt>
                <dd>{templates.to_string()}</dd>
                <dt>"FHIRconnect files"</dt>
                <dd>{fhirconnect_files.to_string()}</dd>
                <dt>"OMOCL files"</dt>
                <dd>{omocl_files.to_string()}</dd>
                <dt>"Errors"</dt>
                <dd>{errors.to_string()}</dd>
                <dt>"Warnings"</dt>
                <dd>{warnings.to_string()}</dd>
            </dl>
            <p>
                <a href=crate::engine::MAPPINGS>"The mappings overview"</a>
            </p>
        </section>
    }
    .into_any())
}
