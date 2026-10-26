// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Configuration page (T3): the settings no face or connection owns,
//! under three binder tabs, Logging, Mappings and Server and access, each a
//! route.
//!
//! The bridge reads its configuration from its file alone (FerroUI family
//! rule 13), so the page is read-only and says where the values are set in
//! one quiet line, whose last words open "How configuration works". No
//! specification governs the console: our own design.

use std::borrow::Cow;

use ferrobridge_operator_api::configuration::Configuration;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::surface::{Header, page_heading};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::settings::{How, SettingsPage, Source, settings_page};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::app::{CONFIGURATION, page_title};
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{faulted, refused_operator};
use crate::pages::settings::rows::{CORE, LOGGING, MAPPINGS};
use crate::pages::settings::{HOW_DIALOG, Row, file_of, how_dialog, setting};

/// The tabs, each its key, its words and its rows.
const TABS: [(&str, Msg, &[Row]); 3] = [
    ("logging", Msg::TabLogging, &LOGGING),
    ("mappings", Msg::TabMappings, &MAPPINGS),
    ("core", Msg::TabServerAccess, &CORE),
];

/// The page of the tab `key`.
fn tab_href(key: &str) -> String {
    if key == "logging" {
        String::from(CONFIGURATION)
    } else {
        format!("{CONFIGURATION}/{key}")
    }
}

/// The Configuration page, on the tab the route names.
#[must_use]
pub fn page() -> impl IntoView {
    let tab = use_params_map()
        .with_untracked(|params| params.get("tab").unwrap_or_else(|| String::from("logging")));
    if !TABS.iter().any(|(key, _, _)| *key == tab) {
        return crate::pages::errors::not_found().into_any();
    }
    let configuration = Resource::new(crate::load::changes, |_| crate::load::configuration_now());
    view! {
        {page_title(tr(Msg::Configuration))}
        <Suspense>
            {move || {
                let tab = tab.clone();
                Suspend::new(async move {
                    let configuration = configuration.await;
                    drawn(&tab, &configuration)
                })
            }}
        </Suspense>
    }
    .into_any()
}

/// The page's header.
fn header() -> Header {
    Header {
        title: tr(Msg::Configuration).into(),
        subtitle: Some(tr(Msg::ConfigurationSubtitle).into()),
        ..Header::default()
    }
}

/// The page over the running configuration.
fn drawn(tab: &str, configuration: &Loaded<Configuration>) -> AnyView {
    let configuration = match configuration {
        Ok(Outcome::Shown(configuration)) => configuration,
        failed => {
            let heading = page_heading(header(), ActionGroup::none()).into_any();
            let body = match failed {
                Ok(Outcome::Refused(refusal)) => refused_operator(refusal),
                Err(fault) => faulted(fault, &tab_href(tab)),
                Ok(Outcome::Shown(_)) => ().into_any(),
            };
            return view! { {heading} {body} }.into_any();
        }
    };
    let rows = TABS
        .iter()
        .find(|(key, _, _)| *key == tab)
        .map_or(&[][..], |(_, _, rows)| *rows);
    let page = settings_page(SettingsPage {
        header: header(),
        actions: ActionGroup::none(),
        source: Source::File {
            file: file_of(configuration),
            how: How::Dialog(Cow::Borrowed(HOW_DIALOG)),
        },
        tabs: Some(TabRow {
            label: tr(Msg::ConfigurationSections),
            tabs: TABS
                .iter()
                .map(|(key, words, _)| Tab::new(*key, tab_href(key), tr(*words)))
                .collect(),
            current: tab.to_owned(),
        }),
        settings: rows.iter().map(|row| setting(configuration, row)).collect(),
        replica: None,
    });
    view! {
        {page}
        {how_dialog()}
    }
    .into_any()
}
