// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OMOP ETL: a detail page (T2) of what a run does.
//!
//! Overview says what a run reads, maps and writes, and how runs are
//! started; Runs lists the runs the bridge schema keeps, each linked to its
//! report ([`run`]); Settings shows the ETL's running configuration. The ETL
//! is a batch of its own process (`ferrobridge etl run`), which keeps each
//! finished run's report for this page to read.

pub mod run;

use ferrobridge_operator_api::configuration::Configuration;
use ferrobridge_operator_api::etl::Runs;
use ferrobridge_operator_api::health::Info;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{Header, definitions, panel_sections};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};

use crate::app::{CONNECTIONS, Drawn, MAPPINGS, OMOP};
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::markup::{code, link, t};
use crate::model::{Loaded, Outcome};
use crate::pages::facade::{indicator, lane_on};
use crate::pages::lane::Lane;
use crate::pages::parts::section;
use crate::pages::settings::rows;
use crate::visit::Visit;
use ferrohealth_health::document::Readiness;
use ferrohealth_ui::view::template::list::list_empty;

/// The tabs, each its key and its words.
const TABS: [(&str, Msg); 3] = [
    ("overview", Msg::TabOverview),
    ("runs", Msg::TabRuns),
    ("settings", Msg::TabSettings),
];

/// The page; its header says whether the ETL is configured.
pub(crate) const LANE: Lane = Lane {
    path: OMOP,
    title: Msg::OmopEtl,
    tabs: &TABS,
    on: Msg::OmopSubtitle,
    off: Msg::OmopSubtitle,
};

/// The OMOP ETL, on the tab the route names.
pub async fn page(visit: &Visit) -> Drawn {
    let Some((tab, info, readiness, configuration)) = LANE.open(visit).await else {
        return crate::pages::errors::not_found(visit);
    };
    let runs = match &info {
        Ok(Outcome::Shown(shown)) if tab == "runs" && lane_on(shown, "etl") => {
            Some(visit.reads.etl_runs().await)
        }
        _ => None,
    };
    Drawn::shell(
        visit.tr(Msg::OmopEtl),
        drawn(
            &visit.locale,
            &tab,
            &info,
            &readiness,
            (configuration.as_ref(), runs.as_ref()),
        ),
    )
}

/// The page over the lane, the CDM database's readiness and, on Settings,
/// the running configuration, and on Runs the runs the bridge keeps.
fn drawn(
    l: &Locale,
    tab: &str,
    info: &Loaded<Info>,
    readiness: &Loaded<Readiness>,
    (configuration, runs): (Option<&Loaded<Configuration>>, Option<&Loaded<Runs>>),
) -> Html {
    let info = match LANE.shown(l, info, tab) {
        Ok(info) => info,
        Err(page) => return page,
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
        return LANE.framed(
            l,
            tab,
            header,
            crate::pages::settings::tab(l, configuration, &rows::OMOP, &LANE.href(tab)),
        );
    }
    if let Some(runs) = runs.filter(|_| on && tab == "runs") {
        return LANE.framed(l, tab, header, run::listing(l, runs));
    }
    if !on {
        return LANE.framed(
            l,
            tab,
            header,
            list_empty(
                tr(l, Msg::OmopOff),
                Some((tr(l, Msg::SeeItsSettings), LANE.href("settings"))),
            ),
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
        (
            tr(l, Msg::RunReports),
            link(LANE.href("runs"), tr(l, Msg::RunReportsValue)),
        ),
    ]);
    detail_page(
        l,
        DetailPage {
            header,
            actions: ActionGroup::none(),
            stepper: None,
            tabs: Some(LANE.tab_row(l, tab, |_, drawn| Some(drawn))),
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
