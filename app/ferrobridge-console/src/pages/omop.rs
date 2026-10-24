// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OMOP ETL: a detail page (T2) of what a run does.
//!
//! Overview says what a run reads, maps and writes, and how runs are
//! started; Settings shows the ETL's running configuration. The
//! ETL is a batch of its own process
//! (`ferrobridge etl run`), so the page says how it runs rather than what
//! a run did; the run reports are planned work (#612).

use ferrobridge_operator_api::configuration::Configuration;
use ferrobridge_operator_api::health::Info;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{Header, definitions, panel_sections};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::app::{CONNECTIONS, MAPPINGS, OMOP, page_title};
use crate::load::Ready;
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::model::{Loaded, Outcome};
use crate::pages::facade::{indicator, lane_on};
use crate::pages::parts::{faulted, quiet, refused, section};
use crate::pages::settings::rows;

/// The tabs, each its key and its words.
const TABS: [(&str, Msg); 2] = [
    ("overview", Msg::TabOverview),
    ("settings", Msg::TabSettings),
];

/// The page of the tab `key`.
fn tab_href(key: &str) -> String {
    if key == "overview" {
        String::from(OMOP)
    } else {
        format!("{OMOP}/{key}")
    }
}

/// The OMOP ETL, on the tab the route names.
#[must_use]
pub fn page() -> impl IntoView {
    let tab = use_params_map().with_untracked(|params| {
        params
            .get("tab")
            .unwrap_or_else(|| String::from("overview"))
    });
    if !TABS.iter().any(|(key, _)| *key == tab) {
        return crate::pages::errors::not_found().into_any();
    }
    view! {
        {page_title(tr(Msg::OmopEtl))}
        <Suspense>
            {move || {
                let tab = tab.clone();
                Suspend::new(async move {
                    let info = crate::load::info().await;
                    let readiness = crate::load::readiness().await;
                    let configuration = if tab == "settings" {
                        Some(crate::load::configuration().await)
                    } else {
                        None
                    };
                    drawn(&tab, &info, &readiness, configuration.as_ref())
                })
            }}
        </Suspense>
    }
    .into_any()
}

/// The tab row with `tab` current.
fn tabs(tab: &str) -> TabRow {
    TabRow {
        label: tr(Msg::OmopEtl),
        tabs: TABS
            .iter()
            .map(|(key, words)| Tab::new(*key, tab_href(key), tr(*words)))
            .collect(),
        current: tab.to_owned(),
    }
}

/// The page over the lane, the CDM database's readiness and, on Settings,
/// the running configuration.
fn drawn(
    tab: &str,
    info: &Loaded<Info>,
    readiness: &Loaded<Ready>,
    configuration: Option<&Loaded<Configuration>>,
) -> AnyView {
    let info = match info {
        Ok(Outcome::Shown(info)) => info,
        Ok(Outcome::Refused(refusal)) => return refused(refusal),
        Err(fault) => return faulted(fault, &tab_href(tab)),
    };
    let on = lane_on(info, "etl");
    let header = Header {
        title: tr(Msg::OmopEtl).into(),
        subtitle: Some(tr(Msg::OmopSubtitle).into()),
        subtitle_view: None,
        crumbs: Vec::new(),
        mono: false,
        badge: Some(
            badge(
                if on { Tone::Ok } else { Tone::Neutral },
                tr(if on {
                    Msg::StateConfigured
                } else {
                    Msg::StateNotConfigured
                }),
            )
            .into_any(),
        ),
    };
    if let Some(configuration) = configuration.filter(|_| tab == "settings") {
        return detail_page(DetailPage {
            header,
            actions: ActionGroup::none(),
            stepper: None,
            tabs: Some(tabs(tab)),
            body: crate::pages::settings::tab(configuration, &rows::OMOP, &tab_href(tab)),
        });
    }
    if !on {
        return detail_page(DetailPage {
            header,
            actions: ActionGroup::none(),
            stepper: None,
            tabs: Some(tabs(tab)),
            body: quiet(
                tr(Msg::OmopOff),
                Some((tr(Msg::SeeItsSettings), tab_href("settings"))),
            ),
        });
    }
    let database = match readiness {
        Ok(Outcome::Shown((_, readiness))) => indicator(readiness, "cdm"),
        Ok(Outcome::Refused(_)) | Err(_) => String::from("–").into_any(),
    };
    let flow = definitions(vec![
        (
            tr(Msg::RunReads),
            view! { <a href=format!("{CONNECTIONS}/cdr")>{tr(Msg::RunReadsValue)}</a> }.into_any(),
        ),
        (
            tr(Msg::RunMaps),
            view! { <a href=format!("{MAPPINGS}/omocl")>{tr(Msg::RunMapsValue)}</a> }.into_any(),
        ),
        (
            tr(Msg::RunWrites),
            view! { <a href=format!("{CONNECTIONS}/cdm")>{tr(Msg::UpstreamCdm)}</a> }.into_any(),
        ),
        (tr(Msg::RunDatabase), database),
    ])
    .into_any();
    let runs = definitions(vec![
        (
            tr(Msg::HowItRuns),
            view! { <code>"ferrobridge etl run"</code> }.into_any(),
        ),
        (tr(Msg::RunReports), tr(Msg::RunReportsValue).into_any()),
    ])
    .into_any();
    detail_page(DetailPage {
        header,
        actions: ActionGroup::none(),
        stepper: None,
        tabs: Some(tabs(tab)),
        body: panel_sections(view! {
            {section(tr(Msg::WhatARunDoes), None, flow)}
            {section(tr(Msg::Runs), None, runs)}
        }),
    })
}
