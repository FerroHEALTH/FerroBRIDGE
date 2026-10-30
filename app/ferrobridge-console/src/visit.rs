// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a handler knows of the request it draws a page for.
//!
//! A visit holds the path and the query, the session and its operator, the
//! preferences the cookies keep, the locale they name, the console's own
//! settings and the reads the page makes of the bridge.
//!
//! Every page is drawn from one [`Visit`], once, whole, with the state the
//! server read (FerroUI `docs/consoles.md`, "How a console builds", step
//! 3). The visit reads the preference cookies and never hands the session
//! cookie or the operator's token to a page. No specification governs the
//! console: our own design.

use ferrohealth_ui::i18n::{Arg, Locale};
use ferrohealth_ui::query::Query;
use ferrohealth_ui::theme::{Preferences, cookie};

use crate::bridge::AccessToken;
use crate::load::Reads;
use crate::locale::messages::Msg;
use crate::model::{SessionState, ViewError};
use crate::server::ConsoleState;
use crate::session::COOKIE;

/// The name of the sidebar's cookie after the preference prefix.
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

/// The console's own settings an operator reads on the About page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    /// The bridge's base address.
    pub bridge: String,
    /// The tag of the configured locale.
    pub locale: String,
}

/// One request, as a page reads it.
#[derive(Debug, Clone)]
pub struct Visit {
    /// The address path, such as `/mappings/files`.
    pub path: String,
    /// The address query.
    pub query: Query,
    /// The request's session.
    pub session: SessionState,
    /// The signed-in operator's name, as the provider gave it.
    pub operator: Option<String>,
    /// The preferences and the sidebar's state the cookies keep.
    pub chosen: Chosen,
    /// The page's locale.
    pub locale: Locale,
    /// The locale the browser asks for, which Your preferences names as the
    /// browser's own.
    pub browser: Locale,
    /// The console's settings, for a signed-in operator alone.
    pub settings: Option<Settings>,
    /// Whether the console has an OpenID Provider to sign operators in at.
    pub sign_in: bool,
    /// Whether the request's session cookie names a session that has ended.
    pub ended: bool,
    /// What the page reads from the bridge, each document once.
    pub reads: Reads,
}

impl Visit {
    /// The visit of the request `parts` to the console `state`.
    #[must_use]
    pub fn of(state: &ConsoleState, parts: &http::request::Parts) -> Self {
        let prefix = state
            .sessions()
            .cookie_name(crate::session::PREFERENCE_PREFIX);
        let cookies = preference_cookies(&parts.headers, &prefix);
        let chosen = chosen_in(&cookies, &prefix);
        let browser = crate::locale::negotiated(&parts.headers, state.settings().locale.clone());
        let locale = crate::locale::requested(&chosen.preferences.language, &browser);
        let id = state.sessions().cookie_in(&parts.headers, COOKIE);
        let token = id.as_ref().map(|id| state.sessions().access_token(id));
        let ended = matches!(token, Some(Ok(None)));
        let (session, signed) = match token {
            Some(Ok(Some(token))) => (SessionState::SignedIn, Ok(AccessToken::new(token))),
            None | Some(Ok(None)) => (SessionState::SignedOut, Err(ViewError::SignedOut)),
            Some(Err(error)) => {
                tracing::error!(%error, "the session store could not be read");
                (SessionState::Unreadable, Err(ViewError::Unavailable))
            }
        };
        // NOTE: no specification governs this: our own design; a store that cannot
        // be read names no operator, and the session's state reports that fault itself.
        let operator = (session == SessionState::SignedIn)
            .then(|| {
                id.as_ref()
                    .and_then(|id| state.sessions().operator(id).ok().flatten())
            })
            .flatten();
        let settings = (session == SessionState::SignedIn).then(|| Settings {
            bridge: state.settings().bridge.base.to_string(),
            locale: state.settings().locale.tag(),
        });
        Self {
            path: parts.uri.path().to_owned(),
            query: Query::parse(parts.uri.query().unwrap_or_default()),
            session,
            operator,
            chosen,
            locale,
            browser,
            settings,
            sign_in: state.settings().oidc.is_some(),
            ended,
            reads: Reads::new(state.clone(), signed),
        }
    }

    /// The first value of the query parameter `name`.
    #[must_use]
    pub fn param(&self, name: &str) -> Option<&str> {
        self.query.get_str(name)
    }

    /// The path segment at `index`, counted from the first after the root.
    #[must_use]
    pub fn segment(&self, index: usize) -> Option<&str> {
        self.path
            .split('/')
            .filter(|part| !part.is_empty())
            .nth(index)
    }

    /// The path with its query, as a link back to this very page.
    #[must_use]
    pub fn here(&self) -> String {
        format!("{}{}", self.path, self.query.to_query_string())
    }

    /// The console's message `msg` in the visit's locale.
    #[must_use]
    pub fn tr(&self, msg: Msg) -> String {
        crate::locale::tr(&self.locale, msg)
    }

    /// The console's message `msg` in the visit's locale, with the
    /// placeables `args` filled in.
    #[must_use]
    pub fn tr_with(&self, msg: Msg, args: &[(&'static str, Arg)]) -> String {
        crate::locale::tr_with(&self.locale, msg, args)
    }
}

/// The preference cookies of `headers`, each named `prefix` then the name
/// of a preference or the sidebar's, joined as one `Cookie` header value;
/// the session's cookie shares the prefix and is never among them.
fn preference_cookies(headers: &http::HeaderMap, prefix: &str) -> String {
    // NOTE: RFC 9113 §8.2.3: a request may split its cookies over several
    // headers, which read as one list joined by `; `.
    headers
        .get_all(http::header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .map(str::trim)
        .filter(|pair| {
            pair.split_once('=').is_some_and(|(name, _value)| {
                name.strip_prefix(prefix).is_some_and(|name| {
                    !prefix.is_empty() && (name == RAIL || Preferences::COOKIES.contains(&name))
                })
            })
        })
        .collect::<Vec<_>>()
        .join("; ")
}
