// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Operator sign-in at an OpenID Provider: the OAuth 2.0 authorization code
//! grant with PKCE, run by the console's server half.
//!
//! `GET /login` mints the `state`, the `nonce` and the PKCE verifier, keeps
//! them in the bounded pool of pending sign-ins, and redirects the browser to
//! the provider's authorization endpoint (RFC 6749 §4.1.1, RFC 7636 §4.3,
//! OpenID Connect Core 1.0 §3.1.2.1). `GET /auth/callback` is where the
//! provider sends the operator back: it takes the pending sign-in once, and
//! the relying party of `ferrohealth-smart` (`oidc::RelyingParty`) holds the
//! returned `state` to it (RFC 6749 §10.12), trades the code for the
//! operator's tokens and checks the ID Token; only then is a signed-in
//! session begun. [`refresh`] renews its access token before it expires, and
//! [`logout`] ends it. The browser never sees a
//! token. No specification of the bridge governs sign-in to the console: our
//! own design on RFC 6749, RFC 7636 and OpenID Connect Core 1.0.

use axum::Extension;
use axum::extract::{Query, Request};
use axum::response::{IntoResponse, Response};
use ferrohealth_secret::secret::Secret;
use ferrohealth_smart::oidc::{Callback, OidcError, PendingSignIn};
use ferrohealth_ui::i18n::Locale;
use http::header::{CACHE_CONTROL, LOCATION, SET_COOKIE};
use http::{HeaderMap, HeaderValue, StatusCode};

use crate::locale::messages::Msg;
use crate::locale::{catalog, negotiated, tr_in};
use crate::server::ConsoleState;
use crate::session::{COOKIE, SIGN_IN_COOKIE, SessionError};

pub mod logout;
pub mod refresh;

/// The path of the sign-in route.
pub const LOGIN: &str = crate::app::SIGN_IN;

/// The path of the redirection endpoint the provider sends the operator back
/// to.
pub const CALLBACK: &str = "/auth/callback";

/// Serves `GET /login`: begins a sign-in and redirects to the provider.
pub async fn login(Extension(state): Extension<ConsoleState>, headers: HeaderMap) -> Response {
    let locale = locale_of(&state, &headers);
    let Some(relying_party) = state.relying_party() else {
        return plain(
            StatusCode::SERVICE_UNAVAILABLE,
            &locale,
            Msg::SignInNotConfigured,
        );
    };
    let request = match relying_party.authorization_request() {
        Ok(request) => request,
        Err(error) => {
            tracing::error!(%error, "a sign-in could not be minted");
            return plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                &locale,
                Msg::SignInSessionNotBegun,
            );
        }
    };
    let location = request.url;
    let id = match state.sessions().begin(request.pending) {
        Ok(id) => id,
        Err(error) => return refused(&locale, &error),
    };
    let cookie = state.sessions().sign_in_cookie(&id).to_string();
    let (Ok(location), Ok(cookie)) = (
        HeaderValue::from_str(location.as_str()),
        HeaderValue::from_str(&cookie),
    ) else {
        tracing::error!("the sign-in redirect could not be written as headers");
        return plain(
            StatusCode::INTERNAL_SERVER_ERROR,
            &locale,
            Msg::SignInRedirectUnwritten,
        );
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    headers.insert(LOCATION, location);
    headers.insert(SET_COOKIE, cookie);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Serves `GET /auth/callback`: holds the returned `state` to the pending
/// sign-in the browser's cookie names, and completes it.
pub async fn callback(Extension(state): Extension<ConsoleState>, request: Request) -> Response {
    let (parts, _body) = request.into_parts();
    let headers = parts.headers.clone();
    let locale = locale_of(&state, &headers);
    let query = Query::<Callback>::try_from_uri(&parts.uri)
        .map(|Query(query)| query)
        .unwrap_or_default();
    let Some(id) = state.sessions().cookie_in(&headers, SIGN_IN_COOKIE) else {
        return plain(StatusCode::BAD_REQUEST, &locale, Msg::NoSignInPending);
    };
    let mut response = match state.sessions().take_pending(&id) {
        Ok(Some(pending)) => match redirected(&state, &headers, &pending, &query).await {
            Ok(response) => response,
            Err(Stopped::Page { status, arrived }) => {
                stopped_page(state.clone(), parts, status, arrived).await
            }
            Err(Stopped::Plain(response)) => *response,
        },
        Ok(None) => plain(StatusCode::BAD_REQUEST, &locale, Msg::NoSignInPending),
        Err(error) => return refused(&locale, &error),
    };
    // NOTE: no specification governs this: our own design; the sign-in is
    // spent either way, so the browser is told to drop its cookie.
    if let Ok(removal) =
        HeaderValue::from_str(&state.sessions().sign_in_cookie_removal().to_string())
    {
        response.headers_mut().append(SET_COOKIE, removal);
    }
    response
}

/// A sign-in the console stopped: the sign-in page saying why in its
/// notice, or a plain answer for a redirect that is no sign-in at all.
#[derive(Debug)]
enum Stopped {
    /// The sign-in page in a state of its card.
    Page {
        /// The status the page answers with.
        status: StatusCode,
        /// The state the sign-in card is drawn in.
        arrived: crate::pages::sign_in::Arrived,
    },
    /// A plain answer.
    Plain(Box<Response>),
}

impl Stopped {
    /// A sign-in stopped with `status`, drawn in `state`.
    fn new(status: StatusCode, state: ferrohealth_ui::view::sign_in::State) -> Self {
        Self::Page {
            status,
            arrived: crate::pages::sign_in::Arrived(state),
        }
    }

    /// A sign-in stopped with the plain answer `response`.
    fn plain(response: Response) -> Self {
        Self::Plain(Box::new(response))
    }
}

/// The sign-in page in the state `arrived`, answered with `status`.
async fn stopped_page(
    state: ConsoleState,
    parts: http::request::Parts,
    status: StatusCode,
    arrived: crate::pages::sign_in::Arrived,
) -> Response {
    let mut response = crate::server::render(state, parts, move || {
        leptos::context::provide_context(arrived.clone());
    })
    .await;
    *response.status_mut() = status;
    response
}

/// The notice of a stopped sign-in in `locale`: `text`, and `detail` for
/// whoever runs the console, with a way to try again when it helps.
fn stopped_notice(
    locale: &Locale,
    text: Msg,
    detail: Msg,
    retry: bool,
) -> ferrohealth_ui::view::sign_in::Notice {
    ferrohealth_ui::view::sign_in::Notice {
        text: tr_in(catalog(), locale, text),
        detail: Some(tr_in(catalog(), locale, detail)),
        retry: retry.then(|| ferrohealth_ui::view::sign_in::Retry {
            label: tr_in(catalog(), locale, Msg::TryAgain),
            href: String::from(LOGIN),
        }),
    }
}

/// The answer to a redirect back for the pending sign-in `pending`: the
/// code exchanged, the ID Token checked, and a signed-in session begun.
///
/// # Errors
/// Returns the [`Stopped`] sign-in the page says in its notice: refused by
/// the provider or its ID Token (`401`), not completed at the provider
/// (`502`), or a full session pool (`503`).
async fn redirected(
    state: &ConsoleState,
    headers: &HeaderMap,
    pending: &PendingSignIn,
    query: &Callback,
) -> Result<Response, Stopped> {
    use ferrohealth_ui::view::sign_in::State;
    let locale = &locale_of(state, headers);
    let signed_in = exchanged(state, pending, query, locale).await?;
    // NOTE: no specification governs this: our own design; a new sign-in ends
    // the session the browser held, so signing in again never fills the pool.
    if let Some(replaced) = state.sessions().cookie_in(headers, COOKIE)
        && let Err(error) = state.sessions().end(&replaced)
    {
        return Ok(refused(locale, &error));
    }
    let id = match state.sessions().establish(signed_in) {
        Ok(id) => id,
        Err(SessionError::Full) => {
            return Err(Stopped::new(
                StatusCode::SERVICE_UNAVAILABLE,
                State::Full(stopped_notice(
                    locale,
                    Msg::SignInFull,
                    Msg::SessionsFull,
                    false,
                )),
            ));
        }
        Err(error) => return Ok(refused(locale, &error)),
    };
    let Ok(cookie) = HeaderValue::from_str(&state.sessions().session_cookie(&id).to_string())
    else {
        tracing::error!("the session cookie could not be written as a header");
        return Ok(plain(
            StatusCode::INTERNAL_SERVER_ERROR,
            locale,
            Msg::SessionNotBegun,
        ));
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    headers.insert(LOCATION, HeaderValue::from_static("/"));
    headers.insert(SET_COOKIE, cookie);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// The operator the provider's redirect `query` signs in, its code exchanged
/// by the relying party and its ID Token checked.
///
/// # Errors
/// Returns the [`Stopped`] sign-in the page says in its notice, or a plain
/// answer as `Ok` in its `Err`: a `state` that is not the pending sign-in's
/// (`400`), a redirect with no code (`400`), a provider or an ID Token that
/// refused the operator (`401`), an exchange that could not be completed
/// (`502`), and no configured provider (`503`).
async fn exchanged(
    state: &ConsoleState,
    pending: &PendingSignIn,
    query: &Callback,
    locale: &Locale,
) -> Result<crate::session::SignedIn, Stopped> {
    use ferrohealth_ui::view::sign_in::State;
    let Some(relying_party) = state.relying_party() else {
        return Err(Stopped::plain(plain(
            StatusCode::SERVICE_UNAVAILABLE,
            locale,
            Msg::SignInNotConfigured,
        )));
    };
    match relying_party.exchange(pending, query).await {
        Ok(signed_in) => Ok(crate::session::SignedIn {
            operator: signed_in.claims.display_name().map(str::to_owned),
            access_token: signed_in.access_token.to_secret_string(),
            expires_in: signed_in.expires_in,
            refresh_token: signed_in
                .refresh_token
                .as_ref()
                .map(Secret::to_secret_string),
            id_token: Some(signed_in.id_token.to_secret_string()),
        }),
        // NOTE: RFC 6749 §10.12: a redirect whose `state` is not the one the
        // request carried is refused before anything else it says is read.
        Err(OidcError::StateMismatch) => Err(Stopped::plain(plain(
            StatusCode::BAD_REQUEST,
            locale,
            Msg::SignInStateMismatch,
        ))),
        Err(OidcError::Authorization { .. }) => Err(Stopped::new(
            StatusCode::UNAUTHORIZED,
            State::Rejected(stopped_notice(
                locale,
                Msg::SignInRejected,
                Msg::ProviderDidNotSignIn,
                false,
            )),
        )),
        Err(OidcError::NoCode) => Err(Stopped::plain(plain(
            StatusCode::BAD_REQUEST,
            locale,
            Msg::RedirectCarriesNoCode,
        ))),
        Err(error) if error.refuses_the_operator() => {
            tracing::warn!(error = %error, "a sign-in's ID Token was refused");
            Err(Stopped::new(
                StatusCode::UNAUTHORIZED,
                State::Rejected(stopped_notice(
                    locale,
                    Msg::SignInRejected,
                    Msg::IdTokenDidNotVerify,
                    false,
                )),
            ))
        }
        Err(error) => {
            tracing::error!(
                error = %crate::command::chain(&error),
                "a sign-in could not be completed at the OpenID Provider"
            );
            Err(Stopped::new(
                StatusCode::BAD_GATEWAY,
                State::Refused(stopped_notice(
                    locale,
                    Msg::SignInRefused,
                    Msg::SignInNotCompleted,
                    true,
                )),
            ))
        }
    }
}

/// The answer to a session store that failed.
fn refused(locale: &Locale, error: &SessionError) -> Response {
    match error {
        SessionError::Full => plain(StatusCode::SERVICE_UNAVAILABLE, locale, Msg::SessionsFull),
        SessionError::Random | SessionError::Poisoned => {
            tracing::error!(%error, "a sign-in session could not be begun");
            plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                locale,
                Msg::SignInSessionNotBegun,
            )
        }
    }
}

/// A plain-text answer that no cache keeps.
fn plain(status: StatusCode, locale: &Locale, msg: Msg) -> Response {
    let text = tr_in(catalog(), locale, msg);
    (status, [(CACHE_CONTROL, "no-store")], text).into_response()
}

/// The locale of a request to a sign-in route with `headers`.
fn locale_of(state: &ConsoleState, headers: &HeaderMap) -> Locale {
    negotiated(headers, state.settings().locale.clone())
}
