// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the server functions of `crate::actions` ask the bridge.
//!
//! Reload the set, the editor's post, and where a discarded draft goes back
//! to. No specification governs the console: our own design.

use http::{HeaderValue, StatusCode, header};
use leptos_axum::ResponseOptions;

use crate::actions::{Edited, EditorPost, Reloaded};
use crate::bridge::AccessToken;
use crate::model::{Outcome, ViewError, loaded};
use crate::server::ConsoleState;

/// The outcome of reloading the set as the operator whose token is `token`.
pub async fn reloaded(state: &ConsoleState, token: &AccessToken) -> Reloaded {
    Reloaded(
        loaded(state.bridge().reload_mappings(token).await).map(|outcome| match outcome {
            Outcome::Shown(answered) => Outcome::Shown(answered.report),
            Outcome::Refused(refusal) => Outcome::Refused(refusal),
        }),
    )
}

/// The answer to the editor's post `post` at `address`, as the operator
/// whose token is `token`: its draft read back with LF line breaks.
pub async fn edited(
    state: &ConsoleState,
    token: &AccessToken,
    address: &str,
    mut post: EditorPost,
) -> Edited {
    // NOTE: HTML Living Standard §4.10.11: a text area submits its value with every
    // line break as CRLF, so the draft is read back with the LF the editor writes.
    post.draft = post.draft.replace("\r\n", "\n");
    let path = editor_path(address, &post);
    let answer = crate::pages::editor::answer::answer(state, token, path.as_deref(), &post).await;
    Edited { post, answer }
}

/// Answers the running server function with `303 See Other` to `page`, a
/// path on the console's own origin.
///
/// `leptos_axum::redirect` answers a call that asks for no HTML with `200`
/// and a header only its own client reads, and the plain post FerroUI's
/// `draw_answers` forwards asks for none, so the status is set here.
///
/// # Errors
/// Returns [`ViewError::Unavailable`] outside a server function's request
/// or for a `page` that is no header value.
pub fn leave_for(page: &str) -> Result<(), ViewError> {
    let response =
        leptos::prelude::use_context::<ResponseOptions>().ok_or(ViewError::Unavailable)?;
    let location = HeaderValue::from_str(page).map_err(|error| {
        tracing::error!(%error, "a discarded draft's page is no header value");
        ViewError::Unavailable
    })?;
    response.insert_header(header::LOCATION, location);
    response.set_status(StatusCode::SEE_OTHER);
    Ok(())
}

/// The page a discarded draft of the editor at `address` goes back to: the
/// file it was opened from, or the mapping set for a new file.
#[must_use]
pub fn discarded(address: &str) -> String {
    editor_path(address, &EditorPost::default()).map_or_else(
        || String::from(crate::app::MAPPINGS),
        |path| crate::pages::parts::file_href(&path),
    )
}

/// The wire path a post of the editor at `address` saves to: the file the
/// address names, or the path the form gives for a new file or a draft of a
/// provisioned one.
fn editor_path(address: &str, post: &EditorPost) -> Option<String> {
    let named = address
        .strip_prefix(crate::app::EDIT)
        .and_then(|rest| rest.strip_prefix('/'))
        .map(str::to_owned);
    let given = post.path.trim();
    if given.is_empty() {
        named
    } else {
        Some(given.to_owned())
    }
}
