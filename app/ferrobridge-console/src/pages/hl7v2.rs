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
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::state::{Mark, state};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{Header, definitions, facts_row, panel_sections};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::app::{HL7V2, page_title};
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::model::{Loaded, Outcome};
use crate::pages::facade::indicator;
use crate::pages::parts::{faulted, quiet, refused, section};
use crate::pages::settings::rows;
use ferrobridge_operator_api::health::Readiness;

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
    let info = Resource::new(crate::load::changes, |_| crate::load::info_now());
    let readiness = Resource::new(crate::load::changes, |_| crate::load::readiness_now());
    let configuration = (tab == "settings")
        .then(|| Resource::new(crate::load::changes, |_| crate::load::configuration_now()));
    view! {
        {page_title(tr(Msg::Hl7v2))}
        <Suspense>
            {move || {
                let tab = tab.clone();
                Suspend::new(async move {
                    let info = info.await;
                    let readiness = readiness.await;
                    let configuration = match configuration {
                        Some(configuration) => Some(configuration.await),
                        None => None,
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
        label: tr(Msg::Hl7v2),
        tabs: TABS
            .iter()
            .map(|(key, words)| Tab::new(*key, tab_href(key), tr(*words)))
            .collect(),
        current: tab.to_owned(),
    }
}

/// The page over the lane, the listener's readiness and, on Settings, the
/// running configuration.
fn drawn(
    tab: &str,
    info: &Loaded<Info>,
    readiness: &Loaded<Readiness>,
    configuration: Option<&Loaded<Configuration>>,
) -> AnyView {
    let info = match info {
        Ok(Outcome::Shown(info)) => info,
        Ok(Outcome::Refused(refusal)) => return refused(refusal),
        Err(fault) => return faulted(fault, &tab_href(tab)),
    };
    let lane = info
        .lanes
        .iter()
        .find(|lane| lane.name == "hl7v2" && lane.enabled);
    let header = |on: bool| Header {
        title: tr(Msg::Hl7v2).into(),
        subtitle: Some(
            tr(if on {
                Msg::Hl7v2OnSubtitle
            } else {
                Msg::Hl7v2OffSubtitle
            })
            .into(),
        ),
        subtitle_view: None,
        crumbs: Vec::new(),
        mono: false,
        badge: Some(if on {
            state(Mark::Ok, tr(Msg::StateOn), None)
        } else {
            badge(Tone::Neutral, tr(Msg::StateOff)).into_any()
        }),
    };
    if let Some(configuration) = configuration.filter(|_| tab == "settings") {
        return detail_page(DetailPage {
            header: header(lane.is_some()),
            actions: ActionGroup::none(),
            stepper: None,
            tabs: Some(tabs(tab)),
            body: crate::pages::settings::tab(configuration, &rows::HL7V2, &tab_href(tab)),
        });
    }
    let Some(lane) = lane else {
        return detail_page(DetailPage {
            header: header(false),
            actions: ActionGroup::none(),
            stepper: None,
            tabs: Some(tabs(tab)),
            body: quiet(
                tr(Msg::Hl7v2Off),
                Some((tr(Msg::SeeItsSettings), tab_href("settings"))),
            ),
        });
    };
    let listening = match readiness {
        Ok(Outcome::Shown(readiness)) => indicator(readiness, "hl7v2-listener"),
        Ok(Outcome::Refused(_)) | Err(_) => String::from("–").into_any(),
    };
    let address = lane.listen.clone().unwrap_or_else(|| String::from("–"));
    let facts = facts_row(vec![
        (
            tr(Msg::FactAddress),
            view! { <code>{address}</code> }.into_any(),
        ),
        (tr(Msg::FactListener), listening),
    ])
    .into_any();
    let mapping = definitions(vec![
        (
            tr(Msg::FactGuide),
            view! { <code>"hl7.fhir.uv.v2mappings"</code> }.into_any(),
        ),
        (
            tr(Msg::FactAcknowledgment),
            tr(Msg::AcknowledgmentValue).into_any(),
        ),
        (tr(Msg::FactCommit), tr(Msg::CommitValue).into_any()),
    ])
    .into_any();
    detail_page(DetailPage {
        header: header(true),
        actions: ActionGroup::none(),
        stepper: None,
        tabs: Some(tabs(tab)),
        body: panel_sections(view! {
            {section(tr(Msg::Listener), None, facts)}
            {section(tr(Msg::Mapping), None, mapping)}
        }),
    })
}
