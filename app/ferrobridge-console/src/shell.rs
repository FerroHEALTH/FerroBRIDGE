// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chrome around every signed-in page, drawn with the `FerroHEALTH`
//! components of `ferrohealth_ui` as the reviewed FerroBRIDGE design lays
//! it out.
//!
//! Every handler draws the shell around its page, the navigation marking
//! the page its path names. The top bar holds the drawer button, the FerroBRIDGE lockup, the search
//! over the running load, the bridge's status pill and the user menu, which
//! alone sets the theme (FerroUI family rule 5). The navigation groups
//! Overview, Mappings, Bridge and Console, folds to its rail from 48 rem up
//! and is the drawer below it; the Mapping set carries the honest count of
//! the running load's errors (R12). The footer names the console, its
//! version and its maker. No specification governs the console: our own
//! design.

use std::borrow::Cow;

use ferrobridge_operator_api::operator::EngineState;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::{Arg, Locale};
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::menu::{ThemeChoice, UserMenu, user_menu};
use ferrohealth_ui::view::nav::{Count, Entry, Group, Rail, Sidebar, sidebar};
use ferrohealth_ui::view::shell::{Footer, Shell, TopBar, brand_mark, lockup, topbar};
use ferrohealth_ui::view::status::{Reach, StatusPill, status_pill};
use openehr_mapping_core::diagnostic::Severity;

use crate::app::{
    ABOUT, CONFIGURATION, CONNECTIONS, DASHBOARD, FACADE, HL7V2, MAPPINGS, MARK_DARK, MARK_LIGHT,
    OMOP, PLAYGROUND, PREFERENCES, PRODUCT, SIGN_OUT, VERSION,
};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::model::{Loaded, Outcome, Refusal, ViewError};
use crate::visit::Visit;

/// The id of the navigation, which the drawer button opens.
pub const SIDEBAR_ID: &str = "sidebar";

/// The id of the user menu, which the operator's button opens.
pub const MENU_ID: &str = "user-menu";

/// The id of the top bar's search field.
pub const SEARCH_ID: &str = "find";

/// What the footer of every page names.
#[must_use]
pub fn footer_names() -> Footer {
    Footer {
        product: Cow::Borrowed(PRODUCT),
        version: Cow::Borrowed(VERSION),
        about: Cow::Borrowed(ABOUT),
    }
}

/// The FerroBRIDGE lockup that leads to the dashboard.
#[must_use]
pub fn console_lockup(locale: &Locale) -> Html {
    lockup(
        DASHBOARD,
        brand_mark(MARK_LIGHT, MARK_DARK),
        "FerroBRIDGE",
        tr(locale, Msg::Console),
    )
}

/// Returns the signed-in page around `content`, with the docked bar `dock`
/// and the toast region `toasts`: the top bar, the navigation and the
/// footer, each part reading the bridge as it is drawn.
pub async fn shell(visit: &Visit, content: Html, dock: Option<Html>, toasts: Html) -> Html {
    let locale = &visit.locale;
    let words = if visit.path == crate::pages::search::SEARCH {
        visit.param("q").unwrap_or_default().to_owned()
    } else {
        String::new()
    };
    let info = visit.reads.info().await;
    let state = visit.reads.running().await;
    let bar = top_bar(visit, &info, &words);
    let nav = navigation(locale, &visit.path, visit.chosen.rail, &state);
    ferrohealth_ui::view::shell::shell(
        locale,
        Shell {
            topbar: bar,
            banner: None,
            sidebar: nav,
            content,
            dock,
            toasts,
            footer: footer_names(),
        },
    )
}

/// The top bar of the shell drawn for `visit`, with the bridge's state from
/// `info` and the search's `words`.
fn top_bar(
    visit: &Visit,
    info: &Loaded<ferrobridge_operator_api::health::Info>,
    words: &str,
) -> Html {
    let locale = &visit.locale;
    let operator = visit
        .operator
        .clone()
        .unwrap_or_else(|| crate::app::unnamed_operator(locale));
    let menu = user_menu(
        locale,
        UserMenu {
            id: MENU_ID,
            label: operator.clone(),
            name: Some(operator.clone()),
            who: operator,
            line: tr(locale, Msg::OperatorLine),
            preferences: String::from(PREFERENCES),
            theme: ThemeChoice {
                current: visit.chosen.preferences.theme,
                action: String::from(crate::app::PREFERENCE),
                back: visit.path.clone(),
            },
            choices: Vec::new(),
            links: Vec::new(),
            shortcuts: None,
            sign_out: Some(String::from(SIGN_OUT)),
        },
    );
    let pill = status_pill(pill(locale, info));
    let search = crate::pages::search::search_box(locale, words);
    topbar(
        locale,
        TopBar {
            sidebar: SIDEBAR_ID,
            lockup: console_lockup(locale),
            search: Some(search),
            end: pill.then(menu),
        },
    )
}

/// The status pill for the bridge's `info`: up with its version, refused,
/// unreachable, or unknown.
#[must_use]
pub fn pill(locale: &Locale, info: &Loaded<ferrobridge_operator_api::health::Info>) -> StatusPill {
    match info {
        Ok(Outcome::Shown(info)) => StatusPill {
            reach: Reach::Up,
            words: tr(locale, Msg::BridgeUp),
            detail: Some(tr_with(
                locale,
                Msg::BridgeVersion,
                &[("version", Arg::from(info.version.as_str()))],
            )),
        },
        Ok(Outcome::Refused(Refusal::NotAuthenticated)) => StatusPill {
            reach: Reach::Refused,
            words: tr(locale, Msg::BridgeRefused401),
            detail: None,
        },
        Ok(Outcome::Refused(refusal)) => StatusPill {
            reach: Reach::Unknown,
            words: tr(locale, Msg::BridgeUnknown),
            detail: Some(refusal.text(locale)),
        },
        Err(ViewError::Unreachable) => StatusPill {
            reach: Reach::Unreachable,
            words: tr(locale, Msg::BridgeUnreachable),
            detail: None,
        },
        Err(fault) => StatusPill {
            reach: Reach::Unknown,
            words: tr(locale, Msg::BridgeUnknown),
            detail: Some(fault.text(locale)),
        },
    }
}

/// The count the Mapping set entry carries for the running load `state`:
/// its errors, or a dash with the reason no count is known (R12).
#[must_use]
pub fn error_count(locale: &Locale, state: &Loaded<EngineState>) -> Count {
    match state {
        Ok(Outcome::Shown(state)) => {
            let errors = state
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity() == Severity::Error)
                .count();
            let count = u32::try_from(errors).unwrap_or(u32::MAX);
            Count::Known {
                count,
                words: tr_with(locale, Msg::NavErrors, &[("count", Arg::from(count))]),
            }
        }
        Ok(Outcome::Refused(refusal)) if refusal.not_served() => Count::Unknown {
            why: tr(locale, Msg::NavUnknownOff),
        },
        Ok(Outcome::Refused(_)) | Err(_) => Count::Unknown {
            why: tr(locale, Msg::NavUnknownDown),
        },
    }
}

/// The navigation of the shell drawn at `path`, folded to its rail when
/// `rail`, with the running load's count.
fn navigation(locale: &Locale, path: &str, rail: bool, state: &Loaded<EngineState>) -> Html {
    let entry = |href: &'static str, label: Msg, glyph: Glyph, exact: bool| Entry {
        href: Cow::Borrowed(href),
        label: Cow::Owned(tr(locale, label)),
        glyph,
        exact,
        count: None,
    };
    let mut set = entry(MAPPINGS, Msg::MappingSet, Glyph::LAYERS, false);
    set.count = Some(error_count(locale, state));
    let groups = vec![
        Group {
            id: "nav-overview",
            title: Cow::Owned(tr(locale, Msg::Overview)),
            entries: vec![entry(
                DASHBOARD,
                Msg::Dashboard,
                Glyph::LAYOUT_DASHBOARD,
                true,
            )],
        },
        Group {
            id: "nav-mappings",
            title: Cow::Owned(tr(locale, Msg::Mappings)),
            entries: vec![
                set,
                entry(PLAYGROUND, Msg::Playground, Glyph::FLASK_CONICAL, false),
            ],
        },
        Group {
            id: "nav-bridge",
            title: Cow::Owned(tr(locale, Msg::Bridge)),
            entries: vec![
                entry(FACADE, Msg::FhirFacade, Glyph::FLAME, false),
                entry(HL7V2, Msg::Hl7v2, Glyph::MESSAGE_SQUARE, false),
                entry(OMOP, Msg::OmopEtl, Glyph::TABLE_2, false),
                entry(CONNECTIONS, Msg::Connections, Glyph::PLUG, false),
            ],
        },
        Group {
            id: "nav-console",
            title: Cow::Owned(tr(locale, Msg::ConsoleGroup)),
            entries: vec![
                entry(CONFIGURATION, Msg::Configuration, Glyph::FILE_TEXT, false),
                entry(ABOUT, Msg::About, Glyph::INFO, true),
            ],
        },
    ];
    sidebar(
        locale,
        Sidebar {
            id: SIDEBAR_ID,
            label: tr(locale, Msg::ConsoleNavigation),
            groups,
            current: path.to_owned(),
            rail: Some(Rail {
                collapsed: rail,
                action: String::from(crate::app::SIDEBAR),
                back: path.to_owned(),
            }),
            nest: None,
        },
    )
}
