// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 v2 face: a detail page (T2) of its MLLP listener.
//!
//! Overview reads the lane `GET /health/info` names and the listener's
//! readiness indicator, and says how the face maps a message; Settings
//! shows the face's running configuration. A face that is off says so in
//! one quiet line on its Overview.

use ferrobridge_operator_api::configuration::Configuration;
use ferrobridge_operator_api::health::Info;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{Header, definitions, facts_row, panel_sections};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};

use crate::app::{Drawn, HL7V2};
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::markup::{code, t};
use crate::model::{Loaded, Outcome};
use crate::pages::facade::indicator;
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
        String::from(HL7V2)
    } else {
        format!("{HL7V2}/{key}")
    }
}

/// The HL7 v2 face, on the tab the route names.
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
        visit.tr(Msg::Hl7v2),
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
        label: tr(l, Msg::Hl7v2),
        tabs: TABS
            .iter()
            .map(|(key, words)| Tab::new(*key, tab_href(key), tr(l, *words)))
            .collect(),
        current: tab.to_owned(),
    }
}

/// The page over the lane, the listener's readiness and, on Settings, the
/// running configuration.
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
    let lane = info
        .lanes
        .iter()
        .find(|lane| lane.name == "hl7v2" && lane.enabled);
    let header = |on: bool| Header {
        title: tr(l, Msg::Hl7v2).into(),
        subtitle: Some(
            tr(
                l,
                if on {
                    Msg::Hl7v2OnSubtitle
                } else {
                    Msg::Hl7v2OffSubtitle
                },
            )
            .into(),
        ),
        subtitle_view: None,
        crumbs: Vec::new(),
        mono: false,
        badge: Some(if on {
            state(Mark::Ok, tr(l, Msg::StateOn), None)
        } else {
            badge(Tone::Neutral, tr(l, Msg::StateOff))
        }),
    };
    if let Some(configuration) = configuration.filter(|_| tab == "settings") {
        return detail_page(
            l,
            DetailPage {
                header: header(lane.is_some()),
                actions: ActionGroup::none(),
                stepper: None,
                tabs: Some(tabs(l, tab)),
                body: crate::pages::settings::tab(l, configuration, &rows::HL7V2, &tab_href(tab)),
            },
        );
    }
    let Some(lane) = lane else {
        return detail_page(
            l,
            DetailPage {
                header: header(false),
                actions: ActionGroup::none(),
                stepper: None,
                tabs: Some(tabs(l, tab)),
                body: quiet(
                    tr(l, Msg::Hl7v2Off),
                    Some((tr(l, Msg::SeeItsSettings), tab_href("settings"))),
                ),
            },
        );
    };
    let listening = match readiness {
        Ok(Outcome::Shown(readiness)) => indicator(l, readiness, "hl7v2-listener"),
        Ok(Outcome::Refused(_)) | Err(_) => t("–"),
    };
    let address = lane.listen.clone().unwrap_or_else(|| String::from("–"));
    let facts = facts_row(vec![
        (tr(l, Msg::FactAddress), code(address)),
        (tr(l, Msg::FactListener), listening),
    ]);
    let mapping = definitions(vec![
        (tr(l, Msg::FactGuide), code("hl7.fhir.uv.v2mappings")),
        (
            tr(l, Msg::FactAcknowledgment),
            t(tr(l, Msg::AcknowledgmentValue)),
        ),
        (tr(l, Msg::FactCommit), t(tr(l, Msg::CommitValue))),
    ]);
    detail_page(
        l,
        DetailPage {
            header: header(true),
            actions: ActionGroup::none(),
            stepper: None,
            tabs: Some(tabs(l, tab)),
            body: panel_sections(section(l, tr(l, Msg::Listener), None, facts).then(section(
                l,
                tr(l, Msg::Mapping),
                None,
                mapping,
            ))),
        },
    )
}
