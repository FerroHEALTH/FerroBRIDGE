// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pages the console draws around `ferrohealth-session`'s sign-in.
//!
//! A sign-in the provider refused, could not complete, or that found the
//! session pool full is the sign-in page saying why in its notice; a
//! redirect that is no sign-in at all is a plain answer from the catalog,
//! and a session store that cannot be read is the fault page. Each is drawn
//! in the request's locale, and the sign-in and the sign-out land on the
//! console's own pages. No specification governs the console: our own
//! design.

use axum::response::{IntoResponse as _, Response};
use ferrohealth_session::paths::{CALLBACK, LOGIN, LOGOUT};
use ferrohealth_session::routes::{Pages, Stopped};
use ferrohealth_smart::oidc::OidcError;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::view::sign_in::{Notice, Retry, State};
use http::header::CACHE_CONTROL;
use http::request::Parts;

use crate::app::{DASHBOARD, SIGNED_OUT};
use crate::locale::messages::Msg;
use crate::locale::{catalog, negotiated, tr_in};
use crate::model::SessionState;
use crate::pages::sign_in::Arrived;
use crate::server::ConsoleState;
use crate::visit::Visit;

/// The console's [`Pages`], in the locale the request negotiates over the
/// console's own.
#[derive(Debug, Clone)]
pub struct SignInPages {
    /// The locale a request falls back to.
    locale: Locale,
}

impl SignInPages {
    /// The pages of a console whose fallback locale is `locale`.
    #[must_use]
    pub const fn new(locale: Locale) -> Self {
        Self { locale }
    }
}

impl Pages for SignInPages {
    fn stopped(&self, parts: Parts, why: Stopped) -> impl Future<Output = Response> + Send {
        let fallback = self.locale.clone();
        async move {
            let status = why.status();
            let locale = negotiated(&parts.headers, fallback);
            let Some(state) = parts.extensions.get::<ConsoleState>().cloned() else {
                return plain(status, &locale, said(&why, parts.uri.path()));
            };
            let card = match &why {
                Stopped::Rejected(OidcError::Authorization { .. }) => State::Rejected(notice(
                    &locale,
                    Msg::SignInRejected,
                    Msg::ProviderDidNotSignIn,
                    false,
                )),
                Stopped::Rejected(_) => State::Rejected(notice(
                    &locale,
                    Msg::SignInRejected,
                    Msg::IdTokenDidNotVerify,
                    false,
                )),
                Stopped::NotCompleted(_) => State::Refused(notice(
                    &locale,
                    Msg::SignInRefused,
                    Msg::SignInNotCompleted,
                    true,
                )),
                Stopped::Full => {
                    State::Full(notice(&locale, Msg::SignInFull, Msg::SessionsFull, false))
                }
                Stopped::Store(_) if !ferrohealth_session::paths::is_own(parts.uri.path()) => {
                    let mut visit = Visit::of(&state, &parts);
                    visit.session = SessionState::Unreadable;
                    let drawn = crate::pages::errors::unreadable(&visit).with_status(status);
                    return crate::app::render(&visit, drawn).await;
                }
                _ => return plain(status, &locale, said(&why, parts.uri.path())),
            };
            let visit = Visit::of(&state, &parts);
            let drawn = crate::pages::sign_in::sign_in_page(&visit, Some(Arrived(card)))
                .with_status(status);
            crate::app::render(&visit, drawn).await
        }
    }

    fn sign_in_page(&self) -> &'static str {
        SIGNED_OUT
    }

    fn signed_out(&self) -> &'static str {
        SIGNED_OUT
    }

    fn home(&self) -> &'static str {
        DASHBOARD
    }

    fn session_gone(&self, parts: &Parts) -> String {
        let locale = negotiated(&parts.headers, self.locale.clone());
        tr_in(catalog(), &locale, Msg::SignInExpired)
    }
}

/// The catalog's words for a sign-in that stopped for `why` at `path`, when
/// it draws no page.
fn said(why: &Stopped, path: &str) -> Msg {
    let path = path.trim_end_matches('/');
    match why {
        Stopped::NotConfigured => Msg::SignInNotConfigured,
        Stopped::NoSignInPending => Msg::NoSignInPending,
        Stopped::StateMismatch => Msg::SignInStateMismatch,
        Stopped::NoCode => Msg::RedirectCarriesNoCode,
        Stopped::Full => Msg::SessionsFull,
        Stopped::Store(_) if path == LOGOUT => Msg::SessionNotEnded,
        Stopped::Store(_) if path == CALLBACK => Msg::SessionNotBegun,
        Stopped::Store(_) if path == LOGIN => Msg::SignInSessionNotBegun,
        Stopped::Rejected(_) => Msg::SignInRejected,
        Stopped::NotCompleted(_) => Msg::SignInNotCompleted,
        _ => Msg::UnreadableBody,
    }
}

/// The notice of a stopped sign-in in `locale`: `text`, and `detail` for
/// whoever runs the console, with a way to try again when it helps.
fn notice(locale: &Locale, text: Msg, detail: Msg, retry: bool) -> Notice {
    Notice {
        text: tr_in(catalog(), locale, text),
        detail: Some(tr_in(catalog(), locale, detail)),
        retry: retry.then(|| Retry {
            label: tr_in(catalog(), locale, Msg::TryAgain),
            href: String::from(LOGIN),
        }),
    }
}

/// A plain-text answer that no cache keeps.
fn plain(status: http::StatusCode, locale: &Locale, msg: Msg) -> Response {
    let text = tr_in(catalog(), locale, msg);
    (status, [(CACHE_CONTROL, "no-store")], text).into_response()
}
