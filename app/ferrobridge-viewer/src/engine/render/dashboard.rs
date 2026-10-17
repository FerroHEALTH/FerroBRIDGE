// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dashboard, drawn on the server.
//!
//! It shows the bridge's build facts and pins (`GET /health/info`), its
//! lanes and listeners, its readiness indicators (`GET /health/readiness`),
//! and the counts of what its running load compiled (`GET /operator/state`),
//! each read into its `ferrobridge_operator_api` document.

use std::collections::BTreeSet;

use ferrobridge_operator_api::health::{Info, Lane, Readiness, State};
use ferrobridge_operator_api::operator::EngineState;
use http::StatusCode;
use leptos::prelude::*;
use openehr_mapping_core::diagnostic::Severity;

use crate::bridge::BridgeError;
use crate::engine::render::parts::{self, Shape};

/// The dashboard's sections for the three answers.
#[must_use]
pub fn render(
    info: &Result<Info, BridgeError>,
    readiness: &Result<(StatusCode, Readiness), BridgeError>,
    state: &Result<EngineState, BridgeError>,
) -> String {
    let info_sections = match info {
        Ok(info) => facts(info),
        Err(error) => parts::failed("Bridge", error),
    };
    let ready = match readiness {
        Ok((status, report)) => indicators(*status, report)
            .unwrap_or_else(|shape| parts::unreadable("Readiness", &shape)),
        Err(error) => parts::failed("Readiness", error),
    };
    let loaded = match state {
        Ok(state) => counts(state),
        Err(error) => parts::failed("Mappings", error),
    };
    view! { <div class="sections">{info_sections} {ready} {loaded}</div> }.to_html()
}

/// The build facts, the lanes and the pins of `info`.
fn facts(info: &Info) -> AnyView {
    let lanes = info.lanes.iter().map(lane).collect_view();
    let pins = info
        .pins
        .iter()
        .map(|pin| {
            view! {
                <tr>
                    <th scope="row">{pin.name.clone()}</th>
                    <td>{pin.version.clone()}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <section class="card">
            <h2>"Bridge"</h2>
            <dl>
                <dt>"Product"</dt>
                <dd>{info.product.clone()}</dd>
                <dt>"Version"</dt>
                <dd>{info.version.clone()}</dd>
                <dt>"Commit"</dt>
                <dd>
                    <code>{info.build.commit_short.clone()}</code>
                </dd>
                <dt>"Built"</dt>
                <dd>{info.build.built_at.clone()}</dd>
                <dt>"Compiler"</dt>
                <dd>{info.build.rustc.clone()}</dd>
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
    .into_any()
}

/// One row of the lanes table.
fn lane(lane: &Lane) -> AnyView {
    let listen = lane
        .listen
        .clone()
        .unwrap_or_else(|| String::from("none of its own"));
    let up = match lane.up {
        None => ("not applicable", "badge tone-neutral"),
        Some(true) => ("up", "badge tone-ok"),
        Some(false) => ("down", "badge tone-danger"),
    };
    let (configured, tone) = if lane.enabled {
        ("on", "badge tone-ok")
    } else {
        ("off", "badge tone-neutral")
    };
    view! {
        <tr>
            <th scope="row">{lane.name.clone()}</th>
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
    .into_any()
}

/// The readiness report the bridge answered with `status`, which must agree
/// with the report's own state.
fn indicators(status: StatusCode, report: &Readiness) -> Result<AnyView, Shape> {
    let (word, tone) = match (report.state, status) {
        (State::Up, StatusCode::OK) => ("Ready", "pill tone-ok"),
        (State::Down, StatusCode::SERVICE_UNAVAILABLE) => ("Not ready", "pill tone-danger"),
        _ => return Err(Shape { path: "state" }),
    };
    let rows = report
        .indicators
        .iter()
        .map(|(name, outcome)| {
            let (word, tone) = match outcome.state {
                State::Up => ("up", "badge tone-ok"),
                State::Down => ("down", "badge tone-danger"),
            };
            view! {
                <tr>
                    <th scope="row">{name.clone()}</th>
                    <td>
                        <span class=tone>{word}</span>
                    </td>
                    <td>{outcome.detail.clone().unwrap_or_default()}</td>
                </tr>
            }
        })
        .collect_view();
    let table = if report.indicators.is_empty() {
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
fn counts(state: &EngineState) -> AnyView {
    let counts = state.counts;
    let models = state
        .contexts
        .iter()
        .flat_map(|context| context.models.iter().map(|model| model.name.as_str()))
        .collect::<BTreeSet<_>>()
        .len();
    let errors = state
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .count();
    let warnings = state.diagnostics.len().saturating_sub(errors);
    view! {
        <section class="card">
            <h2>"Mappings"</h2>
            <dl class="counts">
                <dt>"Contexts loaded"</dt>
                <dd>{format!("{} of {}", counts.programs, counts.contexts)}</dd>
                <dt>"Models"</dt>
                <dd>{models.to_string()}</dd>
                <dt>"Templates"</dt>
                <dd>{counts.templates.to_string()}</dd>
                <dt>"FHIRconnect files"</dt>
                <dd>{counts.fhirconnect_files.to_string()}</dd>
                <dt>"OMOCL files"</dt>
                <dd>{counts.omocl_files.to_string()}</dd>
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
    .into_any()
}
