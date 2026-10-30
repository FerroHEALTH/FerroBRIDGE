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
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::surface::{Header, page_heading};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::settings::{How, SettingsPage, Source, settings_page};

use crate::app::{CONFIGURATION, Drawn};
use crate::locale::messages::Msg;
use crate::locale::tr;
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{faulted, refused_operator};
use crate::pages::settings::rows::{CORE, LOGGING, MAPPINGS};
use crate::pages::settings::{HOW_DIALOG, Row, file_of, how_dialog, setting};
use crate::visit::Visit;

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
pub async fn page(visit: &Visit) -> Drawn {
    let tab = visit.segment(1).unwrap_or("logging").to_owned();
    if !TABS.iter().any(|(key, _, _)| *key == tab) {
        return crate::pages::errors::not_found(visit);
    }
    let configuration = visit.reads.configuration().await;
    Drawn::shell(
        visit.tr(Msg::Configuration),
        drawn(&visit.locale, &tab, &configuration),
    )
}

/// The page's header.
fn header(l: &Locale) -> Header {
    Header {
        title: tr(l, Msg::Configuration).into(),
        subtitle: Some(tr(l, Msg::ConfigurationSubtitle).into()),
        ..Header::default()
    }
}

/// The page over the running configuration.
fn drawn(l: &Locale, tab: &str, configuration: &Loaded<Configuration>) -> Html {
    let configuration = match configuration {
        Ok(Outcome::Shown(configuration)) => configuration,
        failed => {
            let heading = page_heading(l, header(l), ActionGroup::none());
            let body = match failed {
                Ok(Outcome::Refused(refusal)) => refused_operator(l, refusal),
                Err(fault) => faulted(l, fault, &tab_href(tab)),
                Ok(Outcome::Shown(_)) => Html::empty(),
            };
            return heading.then(body);
        }
    };
    let rows = TABS
        .iter()
        .find(|(key, _, _)| *key == tab)
        .map_or(&[][..], |(_, _, rows)| *rows);
    let page = settings_page(
        l,
        SettingsPage {
            header: header(l),
            actions: ActionGroup::none(),
            source: Source::File {
                file: file_of(configuration),
                how: How::Dialog(Cow::Borrowed(HOW_DIALOG)),
            },
            tabs: Some(TabRow {
                label: tr(l, Msg::ConfigurationSections),
                tabs: TABS
                    .iter()
                    .map(|(key, words, _)| Tab::new(*key, tab_href(key), tr(l, *words)))
                    .collect(),
                current: tab.to_owned(),
            }),
            settings: rows
                .iter()
                .map(|row| setting(l, configuration, row))
                .collect(),
            replica: None,
        },
    );
    page.then(how_dialog(l))
}
