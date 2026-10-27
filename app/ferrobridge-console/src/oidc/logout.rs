// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Operator sign-out at `POST /logout`.
//!
//! It ends the server-side session, removes the session cookie, and sends
//! the operator to the provider's end-session endpoint when one is
//! configured (OpenID Connect RP-Initiated Logout 1.0 §2).
//!
//! A sign-out is a `POST`, which the server takes only from the console's own
//! pages, as it takes every request that is not a safe method
//! (`ferrohealth_server::same_origin`). The end-session request is the
//! relying party's of `ferrohealth-smart`.

use axum::Extension;
use axum::response::{IntoResponse, Response};
use ferrohealth_secret::secret::Secret;
use http::header::{CACHE_CONTROL, LOCATION, SET_COOKIE};
use http::{HeaderMap, HeaderValue, StatusCode};

use crate::locale::messages::Msg;
use crate::server::ConsoleState;
use crate::session::COOKIE;

/// The path of the sign-out route.
pub const LOGOUT: &str = crate::app::SIGN_OUT;

/// Serves `POST /logout`: ends the session the browser holds, removes its
/// cookie, and redirects to the provider's end-session endpoint or to the
/// signed-out page.
pub async fn logout(Extension(state): Extension<ConsoleState>, headers: HeaderMap) -> Response {
    let locale = super::locale_of(&state, &headers);
    let Some(relying_party) = state.relying_party() else {
        return super::plain(
            StatusCode::SERVICE_UNAVAILABLE,
            &locale,
            Msg::SignInNotConfigured,
        );
    };
    let sessions = state.sessions();
    let held = sessions.cookie_in(&headers, COOKIE);
    let id_token = match held.map(|id| sessions.end(&id)).transpose() {
        Ok(ended) => ended.flatten(),
        Err(error) => {
            tracing::error!(%error, "a session could not be ended");
            return super::plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                &locale,
                Msg::SessionNotEnded,
            );
        }
    };
    let hint = id_token.map(Secret::from);
    let location = relying_party
        .end_session_url(hint.as_ref(), None)
        .map_or_else(|| String::from(crate::app::SIGNED_OUT), String::from);
    let removal = sessions.session_cookie_removal().to_string();
    let (Ok(location), Ok(removal)) = (
        HeaderValue::from_str(&location),
        HeaderValue::from_str(&removal),
    ) else {
        tracing::error!("the sign-out redirect could not be written as headers");
        return super::plain(
            StatusCode::INTERNAL_SERVER_ERROR,
            &locale,
            Msg::SignOutRedirectUnwritten,
        );
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    headers.insert(LOCATION, location);
    headers.insert(SET_COOKIE, removal);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
