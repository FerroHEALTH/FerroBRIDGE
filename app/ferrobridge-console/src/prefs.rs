// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator's personal preferences: each in a cookie of its own, kept by
//! a plain form post and read when every page is drawn.
//!
//! The theme, the density, reduced motion, the language, the first day of
//! the week and the shortcut hints are the family's preferences
//! (`ferrohealth_ui::theme::Preferences`), each its own cookie named
//! `ferrobridge_console_` and the preference, under the `__Host-` prefix
//! while the cookies are `Secure`. Each cookie is `HttpOnly`,
//! `SameSite=Lax` and lives one year (FerroUI family rule 9), and none
//! holds patient data. The sidebar's rail is one more cookie of the same
//! kind. The server reads them all as it renders, so the first paint is
//! right; the browser keeps no storage. No specification governs this: our
//! own design.

use axum::Extension;
use axum::extract::Form;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use cookie::Cookie;
use ferrohealth_ui::theme::{
    Density, Hints, Language, Motion, Preference as _, Preferences, Theme, WeekStart, cookie,
};
use http::header::SET_COOKIE;
use http::{HeaderValue, StatusCode};
use leptos::context::use_context;

use crate::locale::messages::Msg;
use crate::locale::{catalog, negotiated, tr_in};
use crate::server::ConsoleState;

/// The route every preference posts to, its value under its cookie's name.
pub const PREFERENCE: &str = "/preference";

/// The route Reset to defaults posts to.
pub const RESET: &str = "/preferences/reset";

/// The route the sidebar's Collapse and Expand post to.
pub const SIDEBAR: &str = "/sidebar";

/// The name of the sidebar's cookie after [`crate::session::PREFERENCE_PREFIX`].
pub const RAIL: &str = "sidebar";

/// The preferences and the sidebar's state one request carries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chosen {
    /// The family's preferences.
    pub preferences: Preferences,
    /// Whether the navigation is folded to its rail.
    pub rail: bool,
}

/// The preferences the `Cookie` header value `cookies` holds, each named
/// `prefix` then its cookie's name.
#[must_use]
pub fn chosen_in(cookies: &str, prefix: &str) -> Chosen {
    Chosen {
        preferences: Preferences::read(cookies, prefix),
        rail: cookie(cookies, &format!("{prefix}{RAIL}")) == Some("rail"),
    }
}

/// The preferences of the request being rendered, the defaults where it
/// carries none.
#[must_use]
pub fn requested() -> Chosen {
    let Some(state) = use_context::<ConsoleState>() else {
        return Chosen::default();
    };
    let Some(parts) = use_context::<http::request::Parts>() else {
        return Chosen::default();
    };
    chosen_in(
        &cookies_of(&parts.headers),
        &state
            .sessions()
            .cookie_name(crate::session::PREFERENCE_PREFIX),
    )
}

/// Every `Cookie` header of `headers`, joined as one header value.
fn cookies_of(headers: &HeaderMap) -> String {
    // NOTE: RFC 9113 §8.2.3: a request may split its cookies over several
    // headers, which read as one list joined by `; `.
    headers
        .get_all(http::header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>()
        .join("; ")
}

/// The value of the preference `name` as its cookie keeps it, when `value`
/// is one the console offers for it, and `None` for anything else.
#[must_use]
pub fn preference_value(name: &str, value: &str) -> Option<String> {
    let kept = |choice: Option<&str>| choice.map(str::to_owned);
    if name == Theme::COOKIE {
        kept(Theme::from_cookie(value).map(Theme::key))
    } else if name == Density::COOKIE {
        kept(Density::from_cookie(value).map(Density::key))
    } else if name == Motion::COOKIE {
        kept(Motion::from_cookie(value).map(Motion::key))
    } else if name == WeekStart::COOKIE {
        kept(WeekStart::from_cookie(value).map(WeekStart::key))
    } else if name == Hints::COOKIE {
        kept(Hints::from_cookie(value).map(Hints::key))
    } else if name == Language::COOKIE {
        let offered = |tag: &str| {
            crate::locale::shipped()
                .iter()
                .any(|locale| locale.tag() == tag)
        };
        Language::from_cookie(value)
            .filter(|language| language.tag().is_none_or(offered))
            .map(|language| language.cookie_value().to_owned())
    } else {
        None
    }
}

/// The page `back` names when it is one of the console's own, plain path
/// segments of visible ASCII with no query, and `/` for anything else, so a
/// post never sends the browser off the console.
#[must_use]
pub fn return_path(back: &str) -> &str {
    let plain = back.starts_with('/')
        && !back.starts_with("//")
        && back
            .chars()
            .all(|c| c.is_ascii_graphic() && !matches!(c, '\\' | '?' | '#'))
        && !back
            .split('/')
            .any(|segment| segment == ".." || segment == ".");
    if plain { back } else { "/" }
}

/// Where a preference post returns: the page `back` names, and on Your
/// preferences the same page saying which preference it kept.
#[must_use]
pub fn preference_return(back: &str, name: &str) -> String {
    let page = return_path(back);
    if page.starts_with(crate::app::PREFERENCES) {
        format!("{page}?saved={name}")
    } else {
        page.to_owned()
    }
}

/// The cookie that keeps `value` under the preference `name`.
fn kept_cookie(state: &ConsoleState, name: &str, value: &str) -> Cookie<'static> {
    state.sessions().preference_cookie(name, value)
}

/// The cookie that clears the preference `name`.
fn cleared_cookie(state: &ConsoleState, name: &str) -> Cookie<'static> {
    state.sessions().preference_cookie_removal(name)
}

/// Whether `headers` carry a live signed-in session.
fn signed_in(state: &ConsoleState, headers: &HeaderMap) -> bool {
    state
        .sessions()
        .cookie_in(headers, crate::session::COOKIE)
        .and_then(|id| state.sessions().access_token(&id).ok().flatten())
        .is_some()
}

/// The plain answer `status` with the message `msg` in the request's
/// language.
fn refused(state: &ConsoleState, headers: &HeaderMap, status: StatusCode, msg: Msg) -> Response {
    let locale = negotiated(headers, state.settings().locale.clone());
    (
        status,
        [(http::header::CACHE_CONTROL, "no-store")],
        tr_in(catalog(), &locale, msg),
    )
        .into_response()
}

/// The redirect to `location` that sets each of `cookies`.
fn redirect_with(location: &str, cookies: &[Cookie<'static>]) -> Response {
    let mut response = Redirect::to(location).into_response();
    for kept in cookies {
        match HeaderValue::from_str(&kept.to_string()) {
            Ok(value) => {
                response.headers_mut().append(SET_COOKIE, value);
            }
            Err(_invalid) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
    response
}

/// Serves `POST /preference`: keeps the one preference a plain form post
/// chose in its cookie and sends the browser back to the page it came
/// from, for a signed-in operator.
///
/// The post carries `back` and one preference under its cookie's name; a
/// switch posts `off` before its `on`, and the last value wins.
pub async fn choose(
    Extension(state): Extension<ConsoleState>,
    headers: HeaderMap,
    Form(fields): Form<Vec<(String, String)>>,
) -> Response {
    if !signed_in(&state, &headers) {
        return refused(
            &state,
            &headers,
            StatusCode::UNAUTHORIZED,
            Msg::ViewSignedOut,
        );
    }
    let back = fields
        .iter()
        .rev()
        .find(|(name, _)| name == "back")
        .map_or("/", |(_, back)| back.as_str());
    let chosen = fields
        .iter()
        .filter(|(name, _)| name != "back")
        .collect::<Vec<_>>();
    let Some((name, value)) = chosen.last() else {
        return refused(
            &state,
            &headers,
            StatusCode::BAD_REQUEST,
            Msg::PreferenceUnknown,
        );
    };
    if chosen.iter().any(|(other, _)| other != name) {
        return refused(
            &state,
            &headers,
            StatusCode::BAD_REQUEST,
            Msg::PreferenceUnknown,
        );
    }
    let Some(value) = preference_value(name, value) else {
        return refused(
            &state,
            &headers,
            StatusCode::BAD_REQUEST,
            Msg::PreferenceUnknown,
        );
    };
    redirect_with(
        &preference_return(back, name),
        &[kept_cookie(&state, name, &value)],
    )
}

/// What Reset to defaults and the sidebar post carry.
#[derive(Debug, serde::Deserialize)]
pub struct Back {
    /// The page to return to.
    back: String,
}

/// Serves `POST /preferences/reset` for a signed-in operator.
///
/// It clears every preference cookie and sends the browser back to the page
/// it came from. The sidebar's state is no preference of Your preferences,
/// so it stays.
pub async fn reset(
    Extension(state): Extension<ConsoleState>,
    headers: HeaderMap,
    Form(reset): Form<Back>,
) -> Response {
    if !signed_in(&state, &headers) {
        return refused(
            &state,
            &headers,
            StatusCode::UNAUTHORIZED,
            Msg::ViewSignedOut,
        );
    }
    let cleared = Preferences::COOKIES
        .into_iter()
        .map(|name| cleared_cookie(&state, name))
        .collect::<Vec<_>>();
    redirect_with(return_path(&reset.back), &cleared)
}

/// What the sidebar's Collapse or Expand posts.
#[derive(Debug, serde::Deserialize)]
pub struct RailChoice {
    /// `rail` to fold the navigation, `full` to unfold it.
    sidebar: String,
    /// The page to return to.
    back: String,
}

/// Serves `POST /sidebar`: keeps the sidebar's state in its cookie and
/// sends the browser back to the page it came from, for a signed-in
/// operator.
pub async fn rail(
    Extension(state): Extension<ConsoleState>,
    headers: HeaderMap,
    Form(choice): Form<RailChoice>,
) -> Response {
    if !signed_in(&state, &headers) {
        return refused(
            &state,
            &headers,
            StatusCode::UNAUTHORIZED,
            Msg::ViewSignedOut,
        );
    }
    if !matches!(choice.sidebar.as_str(), "rail" | "full") {
        return refused(
            &state,
            &headers,
            StatusCode::BAD_REQUEST,
            Msg::PreferenceUnknown,
        );
    }
    redirect_with(
        return_path(&choice.back),
        &[kept_cookie(&state, RAIL, &choice.sidebar)],
    )
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::theme::{Density, Theme};

    use super::{chosen_in, preference_return, preference_value, return_path};

    #[test]
    fn a_preference_post_keeps_only_a_value_the_console_offers() {
        assert_eq!(
            Some(String::from("dark")),
            preference_value("theme", "dark")
        );
        assert_eq!(None, preference_value("theme", "sepia"));
        assert_eq!(
            Some(String::from("compact")),
            preference_value("density", "compact")
        );
        assert_eq!(
            Some(String::from("browser")),
            preference_value("language", "browser")
        );
        assert_eq!(Some(String::from("en")), preference_value("language", "en"));
        assert_eq!(None, preference_value("language", "tlh"));
        assert_eq!(None, preference_value("reveal", "10"));
    }

    #[test]
    fn a_return_path_never_leaves_the_console() {
        for path in ["/", "/mappings", "/mappings/files/fhirconnect/a.yml"] {
            assert_eq!(path, return_path(path));
        }
        for path in [
            "",
            "about",
            "//attacker.example.net/",
            "https://attacker.example.net/",
            "/about?x=1",
            "/../login",
            "/a/./b",
            "/\\attacker.example.net",
            "/a b",
        ] {
            assert_eq!("/", return_path(path), "{path}");
        }
        assert_eq!(
            "/preferences/language?saved=language",
            preference_return("/preferences/language", "language")
        );
        assert_eq!("/mappings", preference_return("/mappings", "theme"));
    }

    #[test]
    fn the_cookies_are_read_under_the_consoles_prefix() {
        let prefix = "__Host-ferrobridge_console_";
        let chosen = chosen_in(
            "a=1; __Host-ferrobridge_console_theme=dark; __Host-ferrobridge_console_density=compact; __Host-ferrobridge_console_sidebar=rail",
            prefix,
        );
        assert_eq!(Theme::Dark, chosen.preferences.theme);
        assert_eq!(Density::Compact, chosen.preferences.density);
        assert!(chosen.rail);
        assert_eq!(
            Theme::System,
            chosen_in("ferrobridge_console_theme=dark", prefix)
                .preferences
                .theme,
            "a cookie without the prefix is not the console's"
        );
    }
}
