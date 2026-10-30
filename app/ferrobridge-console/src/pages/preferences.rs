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

use crate::app::{Drawn, PREFERENCES};
use crate::locale::messages::Msg;
use crate::visit::Visit;

/// Your preferences, on the tab the route names.
#[must_use]
pub fn page(visit: &Visit) -> Drawn {
    let section = visit
        .segment(1)
        .map_or(Some(Section::Appearance), Section::parse);
    let Some(section) = section else {
        return crate::pages::errors::not_found(visit);
    };
    let routes = SectionRoutes {
        appearance: String::from(PREFERENCES),
        language: format!("{PREFERENCES}/language"),
        keyboard: format!("{PREFERENCES}/keyboard"),
    };
    let page = preferences_page(
        &visit.locale,
        PreferencesPage {
            section,
            routes,
            reading: false,
            preferences: visit.chosen.preferences.clone(),
            action: String::from(crate::app::PREFERENCE),
            saved: visit.param("saved").map(str::to_owned),
            accent: visit.tr(Msg::AccentIndigo),
            languages: crate::locale::shipped(),
            browser: visit.browser.clone(),
            week_from_language: Weekday::Monday,
            shortcuts: None,
            reset: String::from(crate::app::RESET),
            own: Vec::new(),
        },
    );
    Drawn::shell(visit.tr(Msg::YourPreferences), page)
}
