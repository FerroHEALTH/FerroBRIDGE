// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The light, dark and system themes, and the server function that keeps
//! the operator's choice.
//!
//! The choice lives in a cookie the server reads while it renders, and the
//! server writes it as `data-theme` on `<html>`, so a page arrives in the
//! theme it is read in before any script runs, and the browser keeps no
//! storage of its own (the Leptos book, `ssr/22_life_cycle`). `system`
//! follows the operating system through `prefers-color-scheme`, which the
//! stylesheet reads. No specification governs this: our own design.

use leptos::prelude::*;

use crate::model::{Outcome, ViewError};

/// Keeps the signed-in operator's theme choice `theme` in its cookie.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, and
/// [`ViewError::Unavailable`] when the console cannot set the cookie; a key
/// that names no theme is an [`Outcome::Refused`].
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        clippy::unused_async,
        reason = "the server_fn #[server] macro copies this onto the items it writes, of which only the server body sets a cookie and awaits nothing (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
#[server(endpoint = "theme")]
pub async fn set_theme(
    /// The key of the theme chosen.
    theme: String,
    /// The page a plain form post returns to, which the console sends back
    /// to only when it is one of its own pages ([`return_path`]).
    back: Option<String>,
) -> Result<Outcome<()>, ViewError> {
    server::keep(&theme, back.as_deref())
}

/// The console's own pages, each route of its router, the places a theme
/// post returns to.
const PAGES: [&str; 8] = [
    "/",
    crate::app::ABOUT,
    crate::engine::MAPPINGS,
    crate::engine::CAPABILITY,
    crate::engine::FILES,
    crate::tools::PLAYGROUND,
    crate::tools::VALIDATE,
    crate::editor::EDIT,
];

/// The pages whose route takes a file path after them, which a theme post
/// returns to with that path.
const UNDER: [&str; 2] = [crate::engine::FILES, crate::editor::EDIT];

/// The page `path` names when it is one of the console's own pages, and
/// `None` for anything else, so a post never redirects off the console.
///
/// Each route of the console's router is returned to as it is, and a page
/// below `/files` or `/edit` when what follows is plain path segments of
/// visible ASCII.
#[must_use]
pub fn return_path(path: &str) -> Option<&str> {
    if PAGES.contains(&path) {
        return Some(path);
    }
    let rest = UNDER
        .into_iter()
        .find_map(|page| path.strip_prefix(page)?.strip_prefix('/'))?;
    let plain = rest.split('/').all(|segment| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && segment
                .chars()
                .all(|c| c.is_ascii_graphic() && !matches!(c, '?' | '#' | '\\'))
    });
    plain.then_some(path)
}

/// The server half: the cookie the choice is kept in, and the choice a
/// request carries.
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    use http::HeaderValue;
    use http::header::SET_COOKIE;
    use leptos::context::use_context;

    use crate::locale::{Msg, tr_served};
    use crate::model::{Outcome, Refusal, ViewError};
    use crate::server::ViewerState;
    use crate::session::THEME_COOKIE;
    use ferrohealth_ui::theme::Theme;

    /// Sets the cookie of the theme `key` names on the answer, for a request
    /// with a live signed-in session, and sends a plain form post back to
    /// `back` when it is one of the console's own pages.
    ///
    /// # Errors
    /// As [`crate::theme::set_theme`].
    pub fn keep(key: &str, back: Option<&str>) -> Result<Outcome<()>, ViewError> {
        let (state, _token) = crate::session::request::signed_in()?;
        let Some(theme) = Theme::parse(key) else {
            return Ok(Outcome::Refused(Refusal::Invalid {
                reason: tr_served(Msg::ThemeInvalid),
            }));
        };
        let response =
            use_context::<leptos_axum::ResponseOptions>().ok_or(ViewError::Unavailable)?;
        let cookie = state.sessions().theme_cookie(theme.key()).to_string();
        let value = HeaderValue::from_str(&cookie).map_err(|_invalid| ViewError::Unavailable)?;
        response.append_header(SET_COOKIE, value);
        // NOTE: no specification governs this: our own design; only a plain form
        // post, which asks for HTML, is redirected, and only to the console's own page.
        let plain = use_context::<http::request::Parts>().is_some_and(|parts| {
            parts
                .headers
                .get(http::header::ACCEPT)
                .and_then(|accept| accept.to_str().ok())
                .is_some_and(|accept| accept.contains("text/html"))
        });
        // NOTE: no specification governs this: our own design; a return path is
        // visible ASCII, so one no header can carry is no page to return to.
        let page = back
            .and_then(crate::theme::return_path)
            .and_then(|page| HeaderValue::from_str(page).ok());
        if plain && let Some(page) = page {
            response.insert_header(http::header::LOCATION, page);
            response.set_status(http::StatusCode::SEE_OTHER);
        }
        Ok(Outcome::Shown(()))
    }

    /// The theme the request's cookie names, and the system theme for a
    /// request with none or one that names no theme.
    #[must_use]
    pub fn requested() -> Theme {
        let Some(state) = use_context::<ViewerState>() else {
            return Theme::default();
        };
        let Some(parts) = use_context::<http::request::Parts>() else {
            return Theme::default();
        };
        // NOTE: no specification governs this: our own design; a cookie that
        // names no theme is the choice not made, as a missing one is.
        state
            .sessions()
            .cookie_in(&parts.headers, THEME_COOKIE)
            .and_then(|value| Theme::parse(value.as_str()))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::theme::Theme;

    #[test]
    fn every_theme_reads_back_from_its_key_and_no_other_key_names_one() {
        for theme in Theme::ALL {
            assert_eq!(Some(theme), Theme::parse(theme.key()));
            assert_eq!(Ok(theme), theme.to_string().parse());
        }
        assert_eq!(None, Theme::parse("Dark"));
        assert_eq!(None, Theme::parse("sepia"));
        assert_eq!(Theme::System, Theme::default());
    }

    #[test]
    fn a_return_path_is_one_of_the_consoles_own_pages_and_nothing_else() {
        for path in ["/", "/about"] {
            assert_eq!(Some(path), super::return_path(path), "{path}");
        }
        for path in [
            "/mappings",
            "/capability",
            "/files",
            "/files/fhirconnect/diagnose.context.yml",
            "/playground",
            "/validate",
            "/edit",
            "/edit/omocl/medical_data/cluster/Specimen_v1.yml",
        ] {
            assert_eq!(Some(path), super::return_path(path), "{path}");
        }
        for path in [
            "",
            "about",
            "//attacker.example.net/",
            "https://attacker.example.net/about",
            "/about?x=1",
            "/../about",
            "/\\attacker.example.net",
            "/login",
            "/api/theme",
            "/files/",
            "/files//attacker.example.net",
            "/files/../login",
            "/files/a?b",
            "/files/a\\b",
            "/files/a b",
            "/filesx",
            "/edit/./a.yml",
        ] {
            assert_eq!(None, super::return_path(path), "{path}");
        }
    }
}
