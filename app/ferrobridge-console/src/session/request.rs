// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The session of the request being rendered or answered, read from the
//! context the server provides to every page it draws.

use leptos::context::use_context;

use crate::bridge::AccessToken;
use crate::model::{SessionState, ViewError};
use crate::server::ConsoleState;
use crate::session::COOKIE;

/// The console's state and the operator's access token, for a request that
/// carries a live signed-in session.
///
/// Every read a page makes calls this before it asks the bridge anything.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] when the request carries no live
/// session, and [`ViewError::Unavailable`] when the console's state is
/// missing or its session store is unusable.
pub fn signed_in() -> Result<(ConsoleState, AccessToken), ViewError> {
    let state = use_context::<ConsoleState>().ok_or(ViewError::Unavailable)?;
    let parts = use_context::<http::request::Parts>().ok_or(ViewError::SignedOut)?;
    let id = state
        .sessions()
        .cookie_in(&parts.headers, COOKIE)
        .ok_or(ViewError::SignedOut)?;
    let token = state
        .sessions()
        .access_token(&id)
        .map_err(|error| {
            tracing::error!(%error, "the session store could not be read");
            ViewError::Unavailable
        })?
        .ok_or(ViewError::SignedOut)?;
    Ok((state, AccessToken::new(token)))
}

/// The state of the request's session, which decides the page drawn.
#[must_use]
pub fn state() -> SessionState {
    match signed_in() {
        Ok(_) => SessionState::SignedIn,
        Err(ViewError::SignedOut) => SessionState::SignedOut,
        Err(_) => SessionState::Unreadable,
    }
}

/// The name of the operator of the request's live signed-in session, as the
/// provider gave it.
#[must_use]
pub fn operator() -> Option<String> {
    let state = use_context::<ConsoleState>()?;
    let parts = use_context::<http::request::Parts>()?;
    let id = state.sessions().cookie_in(&parts.headers, COOKIE)?;
    // NOTE: no specification governs this: our own design; a store that
    // cannot be read names no operator, and [`state`] reports that fault itself.
    state.sessions().operator(&id).ok().flatten()
}

/// Whether the console has an OpenID Provider to sign operators in at.
#[must_use]
pub fn configured_sign_in() -> bool {
    use_context::<ConsoleState>().is_some_and(|state| state.settings().oidc.is_some())
}

/// Whether the request carries a session cookie whose session has ended,
/// by its idle or absolute timeout or a refused refresh.
#[must_use]
pub fn ended() -> bool {
    let Some(state) = use_context::<ConsoleState>() else {
        return false;
    };
    let Some(parts) = use_context::<http::request::Parts>() else {
        return false;
    };
    state
        .sessions()
        .cookie_in(&parts.headers, COOKIE)
        .is_some_and(|id| matches!(state.sessions().access_token(&id), Ok(None)))
}
