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
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{Header, definitions, panel_sections};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};

use crate::app::{CONNECTIONS, Drawn, MAPPINGS, OMOP};
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::markup::{code, link, t};
use crate::model::{Loaded, Outcome};
use crate::pages::facade::{indicator, lane_on};
use crate::pages::parts::{faulted, quiet, refused, section};
use crate::pages::settings::rows;
use crate::visit::Visit;
use ferrohealth_health::document::Readiness;

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
pub async fn page(visit: &Visit) -> Drawn {
    let tab = visit.segment(1).unwrap_or("overview").to_owned();
    if !TABS.iter().any(|(key, _)| *key == tab) {
        return crate::pages::errors::not_found(visit);
    }
    let info = visit.reads.info().await;
    let readiness = visit.reads.readiness().await;
    let configuration = if tab == "settings" {
        Some(visit.reads.configuration().await)
    } else {
        None
    };
    Drawn::shell(
        visit.tr(Msg::OmopEtl),
        drawn(
            &visit.locale,
            &tab,
            &info,
            &readiness,
            configuration.as_ref(),
        ),
    )
}

/// The tab row with `tab` current.
fn tabs(l: &Locale, tab: &str) -> TabRow {
    TabRow {
        label: tr(l, Msg::OmopEtl),
        tabs: TABS
            .iter()
            .map(|(key, words)| Tab::new(*key, tab_href(key), tr(l, *words)))
            .collect(),
        current: tab.to_owned(),
    }
}

/// The page over the lane, the CDM database's readiness and, on Settings,
/// the running configuration.
fn drawn(
    l: &Locale,
    tab: &str,
    info: &Loaded<Info>,
    readiness: &Loaded<Readiness>,
    configuration: Option<&Loaded<Configuration>>,
) -> Html {
    let info = match info {
        Ok(Outcome::Shown(info)) => info,
        Ok(Outcome::Refused(refusal)) => return refused(l, refusal),
        Err(fault) => return faulted(l, fault, &tab_href(tab)),
    };
    let on = lane_on(info, "etl");
    let header = Header {
        title: tr(l, Msg::OmopEtl).into(),
        subtitle: Some(tr(l, Msg::OmopSubtitle).into()),
        subtitle_view: None,
        crumbs: Vec::new(),
        mono: false,
        badge: Some(badge(
            if on { Tone::Ok } else { Tone::Neutral },
            tr(
                l,
                if on {
                    Msg::StateConfigured
                } else {
                    Msg::StateNotConfigured
                },
            ),
        )),
    };
    if let Some(configuration) = configuration.filter(|_| tab == "settings") {
        return detail_page(
            l,
            DetailPage {
                header,
                actions: ActionGroup::none(),
                stepper: None,
                tabs: Some(tabs(l, tab)),
                body: crate::pages::settings::tab(l, configuration, &rows::OMOP, &tab_href(tab)),
            },
        );
    }
    if !on {
        return detail_page(
            l,
            DetailPage {
                header,
                actions: ActionGroup::none(),
                stepper: None,
                tabs: Some(tabs(l, tab)),
                body: quiet(
                    tr(l, Msg::OmopOff),
                    Some((tr(l, Msg::SeeItsSettings), tab_href("settings"))),
                ),
            },
        );
    }
    let database = match readiness {
        Ok(Outcome::Shown(readiness)) => indicator(l, readiness, "cdm"),
        Ok(Outcome::Refused(_)) | Err(_) => t("–"),
    };
    let flow = definitions(vec![
        (
            tr(l, Msg::RunReads),
            link(format!("{CONNECTIONS}/cdr"), tr(l, Msg::RunReadsValue)),
        ),
        (
            tr(l, Msg::RunMaps),
            link(format!("{MAPPINGS}/omocl"), tr(l, Msg::RunMapsValue)),
        ),
        (
            tr(l, Msg::RunWrites),
            link(format!("{CONNECTIONS}/cdm"), tr(l, Msg::UpstreamCdm)),
        ),
        (tr(l, Msg::RunDatabase), database),
    ]);
    let runs = definitions(vec![
        (tr(l, Msg::HowItRuns), code("ferrobridge etl run")),
        (tr(l, Msg::RunReports), t(tr(l, Msg::RunReportsValue))),
    ]);
    detail_page(
        l,
        DetailPage {
            header,
            actions: ActionGroup::none(),
            stepper: None,
            tabs: Some(tabs(l, tab)),
            body: panel_sections(
                section(l, tr(l, Msg::WhatARunDoes), None, flow).then(section(
                    l,
                    tr(l, Msg::Runs),
                    None,
                    runs,
                )),
            ),
        },
    )
}
