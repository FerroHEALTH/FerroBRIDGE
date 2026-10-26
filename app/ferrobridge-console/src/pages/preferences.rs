// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Your preferences, behind the operator's name.
//!
//! The library's three tabs (Appearance, Language and region, Keyboard),
//! each preference a plain form post the console keeps in a cookie of its
//! own ([`crate::prefs`]).

use ferrohealth_ui::calendar::Weekday;
use ferrohealth_ui::view::preferences::{
    PreferencesPage, Section, SectionRoutes, preferences_page,
};
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::app::{PREFERENCES, page_title};
use crate::locale::messages::Msg;
use crate::locale::tr;

/// Your preferences, on the tab the route names.
#[must_use]
pub fn page() -> impl IntoView {
    let section = use_params_map().with_untracked(|params| {
        params
            .get("section")
            .map_or(Some(Section::Appearance), |key| Section::parse(&key))
    });
    let Some(section) = section else {
        return crate::pages::errors::not_found().into_any();
    };
    let saved = use_query_map().with_untracked(|query| query.get("saved"));
    let visit = crate::visit::visit();
    let routes = SectionRoutes {
        appearance: String::from(PREFERENCES),
        language: format!("{PREFERENCES}/language"),
        keyboard: format!("{PREFERENCES}/keyboard"),
    };
    let page = preferences_page(PreferencesPage {
        section,
        routes,
        reading: false,
        preferences: visit.chosen().preferences,
        action: String::from(crate::app::PREFERENCE),
        saved,
        accent: tr(Msg::AccentIndigo),
        languages: crate::locale::shipped(),
        browser: visit.browser(),
        week_from_language: Weekday::Monday,
        shortcuts: None,
        reset: String::from(crate::app::RESET),
        own: Vec::new(),
    });
    view! {
        {page_title(tr(Msg::YourPreferences))}
        {page}
    }
    .into_any()
}
