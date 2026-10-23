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
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::model::filled_columns;
use ferrohealth_ui::table::view::Column;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::layout::main_aside;
use ferrohealth_ui::view::status::{badge, status};
use ferrohealth_ui::view::surface::{Stat, card, facts_row, stack, stat_card, stat_grid};
use ferrohealth_ui::view::time::timestamp;
use http::StatusCode;
use leptos::prelude::*;
use openehr_mapping_core::diagnostic::Severity;

use crate::bridge::BridgeError;
use crate::engine::render::parts::{self, Shape};
use crate::locale::{Msg, tr, tr_with};

/// The dashboard's sections for the three answers.
#[must_use]
pub fn render(
    info: &Result<Info, BridgeError>,
    readiness: &Result<(StatusCode, Readiness), BridgeError>,
    state: &Result<EngineState, BridgeError>,
) -> String {
    let tiles = match state {
        Ok(state) => stats(state),
        Err(_) => ().into_any(),
    };
    let loaded = state.as_ref().ok();
    let (built, lanes, pins) = match info {
        Ok(info) => facts(info, loaded),
        Err(error) => (
            parts::failed(&tr(Msg::Bridge), error),
            ().into_any(),
            ().into_any(),
        ),
    };
    // NOTE: no specification governs this: our own design; the counts the tiles
    // do not show sit in the Bridge card, and a load the bridge refused says so.
    let unloaded = match state {
        Ok(_) => ().into_any(),
        Err(error) => parts::failed(&tr(Msg::Mappings), error),
    };
    let ready = match readiness {
        Ok((status, report)) => indicators(*status, report)
            .unwrap_or_else(|shape| parts::unreadable(&tr(Msg::Readiness), &shape)),
        Err(error) => parts::failed(&tr(Msg::Readiness), error),
    };
    let health = stack(view! {
        {lanes}
        {ready}
    });
    stack(view! {
        {tiles}
        {built}
        {unloaded}
        {main_aside(health, pins)}
    })
    .to_html()
}

/// What the running load compiled, counted: the models, and the errors and
/// warnings of its diagnostics.
fn tally(state: &EngineState) -> (usize, usize, usize) {
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
    (models, errors, warnings)
}

/// The four measures across the top, each leading to the page it counts.
fn stats(state: &EngineState) -> AnyView {
    let (models, errors, warnings) = tally(state);
    let mappings = Some(String::from(crate::engine::MAPPINGS));
    let cards = [
        Stat {
            glyph: Glyph::LAYERS,
            value: tr_with(
                Msg::LoadedOf,
                &[
                    ("loaded", state.counts.programs.into()),
                    ("total", state.counts.contexts.into()),
                ],
            ),
            label: tr(Msg::ContextsLoaded).into(),
            href: mappings.clone(),
        },
        Stat {
            glyph: Glyph::FILES,
            value: models.to_string(),
            label: tr(Msg::Models).into(),
            href: mappings.clone(),
        },
        Stat {
            glyph: Glyph::CIRCLE_X,
            value: errors.to_string(),
            label: tr(Msg::Errors).into(),
            href: mappings.clone(),
        },
        Stat {
            glyph: Glyph::TRIANGLE_ALERT,
            value: warnings.to_string(),
            label: tr(Msg::Warnings).into(),
            href: mappings,
        },
    ]
    .into_iter()
    .map(stat_card)
    .collect_view();
    stat_grid(cards).into_any()
}

/// The cards of `info`: the build facts with the files the running load
/// `loaded` read, the lanes, and the pins.
fn facts(info: &Info, loaded: Option<&EngineState>) -> (AnyView, AnyView, AnyView) {
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
        .collect_view()
        .into_any();
    let mut built = vec![
        (tr(Msg::Product), info.product.clone().into_any()),
        (tr(Msg::Version), info.version.clone().into_any()),
        (
            tr(Msg::Commit),
            view! { <code>{info.build.commit_short.clone()}</code> }.into_any(),
        ),
        (tr(Msg::Built), timestamp(&info.build.built_at).into_any()),
        (tr(Msg::Compiler), info.build.rustc.clone().into_any()),
    ];
    if let Some(state) = loaded {
        let counts = state.counts;
        built.extend([
            (tr(Msg::Templates), counts.templates.to_string().into_any()),
            (
                tr(Msg::FhirconnectFiles),
                counts.fhirconnect_files.to_string().into_any(),
            ),
            (
                tr(Msg::OmoclFiles),
                counts.omocl_files.to_string().into_any(),
            ),
        ]);
    }
    let overview = loaded.map(|_| {
        view! {
            <p>
                <a href=crate::engine::MAPPINGS>{tr(Msg::TheMappingsOverview)}</a>
            </p>
        }
    });
    (
        card(
            tr(Msg::Bridge),
            view! {
                {facts_row(built)}
                {overview}
            },
        )
        .into_any(),
        card(tr(Msg::LanesAndListeners), lanes(&info.lanes)).into_any(),
        card(
            tr(Msg::Pins),
            parts::table_unstacked(
                &tr(Msg::Pins),
                &[&tr(Msg::SpecificationOrCrate), &tr(Msg::Version)],
                pins,
            ),
        )
        .into_any(),
    )
}

/// The columns of the lanes table.
fn lane_columns() -> [Column; 3] {
    [
        Column::text("lane", tr(Msg::Lane)),
        Column::text("configured", tr(Msg::Configured)),
        Column::text("listener", tr(Msg::Listener)),
    ]
}

/// The columns of the readiness indicators.
fn indicator_columns() -> [Column; 3] {
    [
        Column::text("indicator", tr(Msg::Indicator)),
        Column::text("state", tr(Msg::State)),
        Column::text("detail", tr(Msg::Detail)),
    ]
}

/// The lanes table; the listener column is drawn only when a lane has a
/// listener of its own.
fn lanes(lanes: &[Lane]) -> AnyView {
    let columns = filled_columns(&lane_columns(), lanes, &[], |lane, key| {
        key != "listener" || lane.listen.is_some()
    });
    let listening = columns.iter().any(|column| column.key == "listener");
    let rows = lanes
        .iter()
        .map(|each| lane(each, listening))
        .collect_view()
        .into_any();
    parts::columns_table(&tr(Msg::LanesAndListeners), &columns, rows)
}

/// One row of the lanes table: with `listening`, the listener of a lane that
/// has one of its own, with whether it accepts connections now.
fn lane(lane: &Lane, listening: bool) -> AnyView {
    let configured = if lane.enabled {
        badge(Tone::Ok, "on")
    } else {
        badge(Tone::Neutral, "off")
    };
    let listener = listening.then(|| {
        let shown = match lane.listen.clone() {
            Some(listen) => {
                let state = match lane.up {
                    Some(true) => badge(Tone::Ok, "up").into_any(),
                    Some(false) => badge(Tone::Bad, "down").into_any(),
                    None => ().into_any(),
                };
                view! {
                    <code>{listen}</code>
                    " "
                    {state}
                }
                .into_any()
            }
            None => view! { <span class="fui-muted">{tr(Msg::NoneOfItsOwn)}</span> }.into_any(),
        };
        view! { <td data-label=tr(Msg::Listener)>{shown}</td> }
    });
    view! {
        <tr>
            <th scope="row">{lane.name.clone()}</th>
            <td data-label=tr(Msg::Configured)>{configured}</td>
            {listener}
        </tr>
    }
    .into_any()
}

/// The readiness report the bridge answered with `status`, which must agree
/// with the report's own state.
fn indicators(status_code: StatusCode, report: &Readiness) -> Result<AnyView, Shape> {
    let verdict = match (report.state, status_code) {
        (State::Up, StatusCode::OK) => status(Tone::Ok, Glyph::CIRCLE_CHECK, &tr(Msg::Ready), None),
        (State::Down, StatusCode::SERVICE_UNAVAILABLE) => {
            status(Tone::Bad, Glyph::CIRCLE_X, &tr(Msg::NotReady), None)
        }
        _ => return Err(Shape { path: "state" }),
    };
    let outcomes: Vec<_> = report.indicators.values().collect();
    let columns = filled_columns(&indicator_columns(), &outcomes, &[], |outcome, key| {
        key != "detail" || outcome.detail.is_some()
    });
    let detailed = columns.iter().any(|column| column.key == "detail");
    let rows = report
        .indicators
        .iter()
        .map(|(name, outcome)| {
            let state = match outcome.state {
                State::Up => badge(Tone::Ok, "up"),
                State::Down => badge(Tone::Bad, "down"),
            };
            let detail = detailed
                .then(|| view! { <td data-label=tr(Msg::Detail)>{outcome.detail.clone()}</td> });
            view! {
                <tr>
                    <th scope="row">{name.clone()}</th>
                    <td data-label=tr(Msg::State)>{state}</td>
                    {detail}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let table = if report.indicators.is_empty() {
        view! { <p>{tr(Msg::BridgeRegistersNoReadinessIndicator)}</p> }.into_any()
    } else {
        parts::columns_table(&tr(Msg::Indicators), &columns, rows)
    };
    Ok(card(
        tr(Msg::Readiness),
        view! {
            <p>{verdict}</p>
            {table}
        },
    )
    .into_any())
}
