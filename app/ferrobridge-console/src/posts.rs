// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The plain form posts a page sends, each answered with the page it came
//! from, drawn again with the outcome.
//!
//! Every action the console offers works with no WebAssembly (FerroUI's
//! interaction contract): Reload the set, the editor's check and save, and
//! the playground's runs are `POST` forms. Each handler checks the session
//! before it asks the bridge anything, sends the request, and draws the
//! page at the same address with the answer in its context, so nothing the
//! operator entered or the bridge answered reaches a URL, the history or a
//! store. A post without a live session is sent back to the page, which
//! then draws the sign-in. The server functions of `crate::actions` run the
//! same work for a page that has hydrated. No specification governs the
//! console: our own design.

use axum::Extension;
use axum::extract::{FromRequest as _, Request};
use axum::response::{IntoResponse, Redirect, Response};

use crate::actions::{Edited, EditorPost, PlaygroundPost, Ran, Reloaded};
use crate::bridge::AccessToken;
use crate::model::{Outcome, loaded};
use crate::server::ConsoleState;

/// The page at `parts`' address again, by `GET`, for a post that carries
/// no live session: the page then draws the sign-in.
fn back_to(parts: &http::request::Parts) -> Response {
    Redirect::to(parts.uri.path()).into_response()
}

/// The request's parts and its body read as the form `T`, or the answer to
/// a body that is no such form.
async fn form<T: serde::de::DeserializeOwned>(
    request: Request,
) -> Result<(http::request::Parts, T), Box<Response>> {
    let (parts, body) = request.into_parts();
    let rebuilt = Request::from_parts(parts.clone(), body);
    match axum::extract::Form::<T>::from_request(rebuilt, &()).await {
        Ok(axum::extract::Form(form)) => Ok((parts, form)),
        Err(rejection) => Err(Box::new(rejection.into_response())),
    }
}

/// The access token of the session `parts` carry, when it is live.
fn token_of(state: &ConsoleState, parts: &http::request::Parts) -> Option<AccessToken> {
    let id = state
        .sessions()
        .cookie_in(&parts.headers, crate::session::COOKIE)?;
    // NOTE: no specification governs this: our own design; a store that
    // cannot be read admits no post, and the page drawn after says why.
    let token = state.sessions().access_token(&id).ok().flatten()?;
    Some(AccessToken::new(token))
}

/// The outcome of reloading the set as the operator whose token is `token`.
pub async fn reloaded(state: &ConsoleState, token: &AccessToken) -> Reloaded {
    Reloaded(
        loaded(state.bridge().reload_mappings(token).await).map(|outcome| match outcome {
            Outcome::Shown(answered) => Outcome::Shown(answered.report),
            Outcome::Refused(refusal) => Outcome::Refused(refusal),
        }),
    )
}

/// Serves `POST /mappings`: reloads the set and draws the Mapping
/// set with the reload's outcome.
pub async fn reload(Extension(state): Extension<ConsoleState>, request: Request) -> Response {
    let (parts, _body) = request.into_parts();
    let Some(token) = token_of(&state, &parts) else {
        return back_to(&parts);
    };
    let reloaded = reloaded(&state, &token).await;
    crate::server::render(state, parts, move || {
        leptos::context::provide_context(reloaded.clone());
    })
    .await
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
    let answer = crate::pages::editor::answer(state, token, path.as_deref(), &post).await;
    Edited { post, answer }
}

/// Serves `POST /mappings/edit/{path}` and `POST /mappings/new`: checks the
/// draft, asks to confirm a save, saves it, or steps back, and draws the
/// editor with the answer.
pub async fn edit(Extension(state): Extension<ConsoleState>, request: Request) -> Response {
    let (parts, post) = match form::<EditorPost>(request).await {
        Ok(read) => read,
        Err(answer) => return *answer,
    };
    let Some(token) = token_of(&state, &parts) else {
        return back_to(&parts);
    };
    if post.decision == "discard" {
        return Redirect::to(&discarded(parts.uri.path())).into_response();
    }
    let edited = edited(&state, &token, parts.uri.path(), post).await;
    crate::server::render(state, parts, move || {
        leptos::context::provide_context(edited.clone());
    })
    .await
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

/// Serves `POST /playground` and its tabs: runs the tab's operation over
/// what the operator entered and draws the playground with the answer.
pub async fn run(Extension(state): Extension<ConsoleState>, request: Request) -> Response {
    let (parts, post) = match form::<PlaygroundPost>(request).await {
        Ok(read) => read,
        Err(answer) => return *answer,
    };
    let Some(token) = token_of(&state, &parts) else {
        return back_to(&parts);
    };
    let answer = crate::pages::playground::answer(&state, &token, parts.uri.path(), &post).await;
    let ran = Ran { post, answer };
    crate::server::render(state, parts, move || {
        leptos::context::provide_context(ran.clone());
    })
    .await
}
