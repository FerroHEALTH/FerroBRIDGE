// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The upstreams the bridge reaches: a list page (T1) of the openEHR CDR,
//! the terminology server and the OMOP CDM database, and a detail page (T2)
//! for each.
//!
//! Each upstream's state is the bridge's readiness indicator for it, and the
//! faces that use it are the lanes `GET /health/info` names. Its address and
//! credentials are the bridge's configuration, which no endpoint answers yet
//! (#1147).

use ferrobridge_operator_api::health::{Info, Readiness, State};
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::surface::{
    Crumb, Header, definitions, page_heading, panel_sections_card,
};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};
use ferrohealth_ui::view::template::list::{ListCard, list_card};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::app::{CONNECTIONS, page_title};
use crate::load::Ready;
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::model::{Loaded, Outcome};
use crate::pages::dashboard::{UPSTREAMS, indicator_name};
use crate::pages::facade::indicator;
use crate::pages::parts::{faulted, quiet, refused, section};

/// The lanes that use the upstream `name`, as `GET /health/info` names
/// them, each its words.
fn users(info: &Info, name: &str) -> Vec<String> {
    let uses = |lane: &str| match name {
        "cdr" => matches!(lane, "facade" | "operations" | "hl7v2" | "etl"),
        "terminology" => matches!(lane, "facade" | "operations" | "hl7v2"),
        "cdm" => lane == "etl",
        _ => false,
    };
    info.lanes
        .iter()
        .filter(|lane| lane.enabled && uses(&lane.name))
        .map(|lane| {
            tr(match lane.name.as_str() {
                "facade" => Msg::FhirFacade,
                "operations" => Msg::Playground,
                "hl7v2" => Msg::Hl7v2,
                _ => Msg::OmopEtl,
            })
        })
        .collect()
}

/// The state of the upstream `name` as its indicator reports it, the tone
/// and the word: the readiness carries an indicator only for an upstream
/// the bridge is configured with.
fn upstream_state(readiness: &Readiness, name: &str) -> AnyView {
    if readiness.indicators.contains_key(name) {
        indicator(readiness, name)
    } else {
        state(Mark::Neutral, tr(Msg::StateNotConfigured), None)
    }
}

/// The page that lists the upstreams.
#[must_use]
pub fn page() -> impl IntoView {
    view! {
        {page_title(tr(Msg::Connections))}
        <Suspense>
            {move || Suspend::new(async move {
                let info = crate::load::info().await;
                let readiness = crate::load::readiness().await;
                list(&info, &readiness)
            })}
        </Suspense>
    }
}

/// The list over the lanes and the readiness.
fn list(info: &Loaded<Info>, readiness: &Loaded<Ready>) -> AnyView {
    let header = page_heading(
        Header {
            title: tr(Msg::Connections).into(),
            subtitle: Some(tr(Msg::ConnectionsSubtitle).into()),
            ..Header::default()
        },
        ActionGroup::none(),
    )
    .into_any();
    let (info, readiness) = match (info, readiness) {
        (Ok(Outcome::Shown(info)), Ok(Outcome::Shown((_, readiness)))) => (info, readiness),
        (Err(fault), _) | (_, Err(fault)) => {
            return view! { {header} {faulted(fault, CONNECTIONS)} }.into_any();
        }
        (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
            return view! { {header} {refused(refusal)} }.into_any();
        }
    };
    let columns = [
        Column::text("connection", tr(Msg::ColConnection)).role(Role::Title),
        Column::text("used", tr(Msg::ColUsedBy)).role(Role::Sub),
        Column::text("state", tr(Msg::ColState)).titled(tr(Msg::StateFromReadiness)),
    ];
    let [_, used, word] = &columns;
    let rows = UPSTREAMS
        .iter()
        .map(|name| {
            let by = users(info, name);
            let by = if by.is_empty() {
                String::from("–")
            } else {
                by.join(", ")
            };
            view! {
                <tr>
                    {title_cell(Some(&format!("{CONNECTIONS}/{name}")), indicator_name(name), None)}
                    {cell(used, by)}
                    {cell(word, upstream_state(readiness, name))}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    let table = data_table(DataTable {
        caption: &tr(Msg::Connections),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
    .into_any();
    let card = list_card(ListCard {
        title: tr(Msg::Connections),
        count: Some(u64::try_from(UPSTREAMS.len()).unwrap_or(u64::MAX).into()),
        order: None,
        tabs: None,
        actions: None,
        body: table,
        pager: None,
    });
    view! {
        {header}
        <div class="fui-list-page">{card}</div>
    }
    .into_any()
}

/// The page of the upstream the route names.
#[must_use]
pub fn connection_page() -> impl IntoView {
    let name = use_params_map()
        .with_untracked(|params| params.get("name"))
        .unwrap_or_default();
    if !UPSTREAMS.contains(&name.as_str()) {
        return crate::pages::errors::not_found().into_any();
    }
    view! {
        {page_title(indicator_name(&name))}
        <Suspense>
            {move || {
                let name = name.clone();
                Suspend::new(async move {
                    let info = crate::load::info().await;
                    let readiness = crate::load::readiness().await;
                    detail(&name, &info, &readiness)
                })
            }}
        </Suspense>
    }
    .into_any()
}

/// What the upstream `name` is, in one sentence.
fn subtitle(name: &str) -> String {
    tr(match name {
        "cdr" => Msg::CdrSubtitle,
        "terminology" => Msg::TerminologySubtitle,
        _ => Msg::CdmSubtitle,
    })
}

/// The detail page of the upstream `name`.
fn detail(name: &str, info: &Loaded<Info>, readiness: &Loaded<Ready>) -> AnyView {
    let (info, readiness) = match (info, readiness) {
        (Ok(Outcome::Shown(info)), Ok(Outcome::Shown((_, readiness)))) => (info, readiness),
        (Err(fault), _) | (_, Err(fault)) => {
            return faulted(fault, &format!("{CONNECTIONS}/{name}"));
        }
        (Ok(Outcome::Refused(refusal)), _) | (_, Ok(Outcome::Refused(refusal))) => {
            return refused(refusal);
        }
    };
    let found = readiness.indicators.get(name);
    let badge = match found {
        Some(found) if found.state == State::Up => state(Mark::Ok, tr(Msg::StateUp), None),
        Some(_) => state(Mark::Bad, tr(Msg::StateUnreachable), None),
        None => state(Mark::Neutral, tr(Msg::StateNotConfigured), None),
    };
    let warning = found
        .filter(|found| found.state == State::Down)
        .map(|found| {
            notice(
                Tone::Bad,
                Announce::Status,
                None,
                found
                    .detail
                    .clone()
                    .unwrap_or_else(|| tr(Msg::NoAnswerDetail)),
            )
            .into_any()
        });
    let by = users(info, name);
    let used = if by.is_empty() {
        quiet(tr(Msg::UsedByNoLane), None)
    } else {
        definitions(
            by.into_iter()
                .map(|lane| (lane, tr(Msg::UsesIt).into_any()))
                .collect(),
        )
        .into_any()
    };
    let states =
        definitions(vec![(tr(Msg::Readiness), upstream_state(readiness, name))]).into_any();
    let body = if found.is_none() {
        quiet(tr(Msg::UpstreamNotConfigured), None)
    } else {
        panel_sections_card(
            indicator_name(name),
            view! {
                {warning.map(|warning| ferrohealth_ui::view::surface::panel_section(None, warning))}
                {section(tr(Msg::State), None, states)}
                {section(tr(Msg::ColUsedBy), None, used)}
            },
        )
    };
    detail_page(DetailPage {
        header: Header {
            title: indicator_name(name).into(),
            subtitle: Some(subtitle(name).into()),
            subtitle_view: None,
            crumbs: vec![Crumb::new(tr(Msg::Connections), CONNECTIONS)],
            mono: false,
            badge: Some(badge),
        },
        actions: ActionGroup::none(),
        stepper: None,
        tabs: None,
        body,
    })
}
