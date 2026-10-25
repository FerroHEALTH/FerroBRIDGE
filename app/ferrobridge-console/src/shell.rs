// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chrome around every signed-in page, drawn with the `FerroHEALTH`
//! components of `ferrohealth_ui` as the reviewed FerroBRIDGE design lays
//! it out.
//!
//! The top bar holds the drawer button, the FerroBRIDGE lockup, the search
//! over the running load, the bridge's status pill and the user menu, which
//! alone sets the theme (FerroUI family rule 5). The navigation groups
//! Overview, Mappings, Bridge and Console, folds to its rail from 48 rem up
//! and is the drawer below it; the Mapping set carries the honest count of
//! the running load's errors (R12). The footer names the console, its
//! version and its maker. No specification governs the console: our own
//! design.

use std::borrow::Cow;

use ferrobridge_operator_api::operator::EngineState;
use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::shell::{Footer, Shell, TopBar, brand_mark, lockup, topbar};
use ferrohealth_ui::view::status::{Reach, StatusPill, status_pill};
use ferrohealth_ui::widget::menu::{ThemeChoice, UserMenu, user_menu};
use ferrohealth_ui::widget::nav::{Count, Entry, Group, Rail, Sidebar, sidebar};
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use openehr_mapping_core::diagnostic::Severity;

use crate::app::{
    ABOUT, CONFIGURATION, CONNECTIONS, DASHBOARD, FACADE, HL7V2, MAPPINGS, MARK_DARK, MARK_LIGHT,
    OMOP, PLAYGROUND, PREFERENCES, PRODUCT, SIGN_OUT, VERSION,
};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::model::{Loaded, Outcome, Refusal, ViewError};

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
pub fn console_lockup() -> AnyView {
    lockup(
        DASHBOARD,
        brand_mark(MARK_LIGHT, MARK_DARK),
        "FerroBRIDGE",
        tr(Msg::Console),
    )
    .into_any()
}

/// Returns the signed-in page around `content`: the top bar, the
/// navigation and the footer, each part reading the bridge as it is drawn.
#[must_use]
pub fn shell(content: AnyView) -> AnyView {
    let path = use_location().pathname.get_untracked();
    let bar = view! {
        <Suspense>
            {move || Suspend::new(async move {
                let info = crate::load::info().await;
                top_bar(&info)
            })}
        </Suspense>
    }
    .into_any();
    let nav = view! {
        <Suspense>
            {
                let path = path.clone();
                move || {
                    let path = path.clone();
                    Suspend::new(async move {
                        let state = crate::load::state().await;
                        navigation(&path, &state)
                    })
                }
            }
        </Suspense>
    }
    .into_any();
    ferrohealth_ui::view::shell::shell(Shell {
        topbar: bar,
        banner: None,
        sidebar: nav,
        content,
        dock: None,
        footer: footer_names(),
    })
}

/// The top bar, with the bridge's state from `info`.
fn top_bar(info: &Loaded<ferrobridge_operator_api::health::Info>) -> AnyView {
    let path = use_location().pathname.get_untracked();
    let words = if path == crate::pages::search::SEARCH {
        leptos_router::hooks::use_query_map()
            .with_untracked(|query| query.get("q"))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let chosen = crate::prefs::requested().preferences;
    let operator = crate::session::request::operator().unwrap_or_else(crate::app::unnamed_operator);
    let menu = user_menu(UserMenu {
        id: MENU_ID,
        label: operator.clone(),
        name: Some(operator.clone()),
        who: operator,
        line: tr(Msg::OperatorLine),
        preferences: String::from(PREFERENCES),
        theme: ThemeChoice {
            current: chosen.theme,
            action: String::from(crate::prefs::PREFERENCE),
            back: path.clone(),
        },
        choices: Vec::new(),
        links: Vec::new(),
        shortcuts: None,
        sign_out: Some(String::from(SIGN_OUT)),
    });
    let pill = status_pill(pill(info));
    let end = view! {
        {pill}
        {menu}
    }
    .into_any();
    let search = crate::pages::search::search_box(&words);
    topbar(TopBar {
        sidebar: SIDEBAR_ID,
        lockup: console_lockup(),
        search: Some(search),
        end,
    })
}

/// The status pill for the bridge's `info`: up with its version, refused,
/// unreachable, or unknown.
#[must_use]
pub fn pill(info: &Loaded<ferrobridge_operator_api::health::Info>) -> StatusPill {
    match info {
        Ok(Outcome::Shown(info)) => StatusPill {
            reach: Reach::Up,
            words: tr(Msg::BridgeUp),
            detail: Some(tr_with(
                Msg::BridgeVersion,
                &[("version", Arg::from(info.version.as_str()))],
            )),
        },
        Ok(Outcome::Refused(Refusal::NotAuthenticated)) => StatusPill {
            reach: Reach::Refused,
            words: tr(Msg::BridgeRefused401),
            detail: None,
        },
        Ok(Outcome::Refused(refusal)) => StatusPill {
            reach: Reach::Unknown,
            words: tr(Msg::BridgeUnknown),
            detail: Some(refusal.text()),
        },
        Err(ViewError::Unreachable) => StatusPill {
            reach: Reach::Unreachable,
            words: tr(Msg::BridgeUnreachable),
            detail: None,
        },
        Err(fault) => StatusPill {
            reach: Reach::Unknown,
            words: tr(Msg::BridgeUnknown),
            detail: Some(fault.text()),
        },
    }
}

/// The count the Mapping set entry carries for the running load `state`:
/// its errors, or a dash with the reason no count is known (R12).
#[must_use]
pub fn error_count(state: &Loaded<EngineState>) -> Count {
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
                words: tr_with(Msg::NavErrors, &[("count", Arg::from(count))]),
            }
        }
        Ok(Outcome::Refused(refusal)) if refusal.not_served() => Count::Unknown {
            why: tr(Msg::NavUnknownOff),
        },
        Ok(Outcome::Refused(_)) | Err(_) => Count::Unknown {
            why: tr(Msg::NavUnknownDown),
        },
    }
}

/// The navigation, the page at `path` marked, with the running load's count.
fn navigation(path: &str, state: &Loaded<EngineState>) -> AnyView {
    let rail = crate::prefs::requested().rail;
    let entry = |href: &'static str, label: Msg, glyph: Glyph, exact: bool| Entry {
        href: Cow::Borrowed(href),
        label: Cow::Owned(tr(label)),
        glyph,
        exact,
        count: None,
    };
    let mut set = entry(MAPPINGS, Msg::MappingSet, Glyph::LAYERS, false);
    set.count = Some(error_count(state));
    let groups = vec![
        Group {
            id: "nav-overview",
            title: Cow::Owned(tr(Msg::Overview)),
            entries: vec![entry(
                DASHBOARD,
                Msg::Dashboard,
                Glyph::LAYOUT_DASHBOARD,
                true,
            )],
        },
        Group {
            id: "nav-mappings",
            title: Cow::Owned(tr(Msg::Mappings)),
            entries: vec![
                set,
                entry(PLAYGROUND, Msg::Playground, Glyph::FLASK_CONICAL, false),
            ],
        },
        Group {
            id: "nav-bridge",
            title: Cow::Owned(tr(Msg::Bridge)),
            entries: vec![
                entry(FACADE, Msg::FhirFacade, Glyph::FLAME, false),
                entry(HL7V2, Msg::Hl7v2, Glyph::MESSAGE_SQUARE, false),
                entry(OMOP, Msg::OmopEtl, Glyph::TABLE_2, false),
                entry(CONNECTIONS, Msg::Connections, Glyph::PLUG, false),
            ],
        },
        Group {
            id: "nav-console",
            title: Cow::Owned(tr(Msg::ConsoleGroup)),
            entries: vec![
                entry(CONFIGURATION, Msg::Configuration, Glyph::FILE_TEXT, false),
                entry(ABOUT, Msg::About, Glyph::INFO, true),
            ],
        },
    ];
    sidebar(Sidebar {
        id: SIDEBAR_ID,
        label: tr(Msg::ConsoleNavigation),
        groups,
        current: path.to_owned(),
        rail: Some(Rail {
            collapsed: rail,
            action: String::from(crate::prefs::SIDEBAR),
            back: path.to_owned(),
        }),
        nest: None,
    })
}
