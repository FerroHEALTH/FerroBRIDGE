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
//! then draws the sign-in. No specification governs the console: our own
//! design.

use axum::Extension;
use axum::extract::{FromRequest as _, Request};
use axum::response::{IntoResponse, Redirect, Response};
use ferrobridge_operator_api::operator::LoadReport;

use crate::bridge::editing::Answered;
use crate::model::{Loaded, Outcome, loaded};
use crate::server::ConsoleState;

/// The outcome of the reload the operator just asked for.
#[derive(Debug, Clone)]
pub struct Reloaded(pub Loaded<LoadReport>);

/// What the editor posts: the draft and what it asks of it.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct EditorPost {
    /// The draft, as the text area holds it.
    pub draft: String,
    /// The digest of the file the draft was opened from; empty for a new
    /// file.
    pub base: String,
    /// The path a new file or a draft of a provisioned file saves under.
    pub path: String,
    /// Why the operator saves it.
    pub reason: String,
    /// What the post asks: `check`, `save`, `confirm` or `back`.
    pub decision: String,
}

/// What the bridge answered a post of the editor.
#[derive(Debug, Clone)]
pub enum EditorAnswer {
    /// The draft as it was posted, asking to confirm its save.
    Confirm,
    /// The draft as it was posted, back from the confirmation.
    Back,
    /// The check's answer.
    Checked(Loaded<Answered>),
    /// The save's answer.
    Saved(Loaded<Answered>),
    /// The post cannot be sent as it stands, for the reason by field.
    Invalid {
        /// The field at fault: `reason` or `path`.
        field: &'static str,
        /// Why.
        said: crate::locale::messages::Msg,
    },
}

/// A post of the editor and what the bridge answered it.
#[derive(Debug, Clone)]
pub struct Edited {
    /// What was posted.
    pub post: EditorPost,
    /// What the bridge answered.
    pub answer: EditorAnswer,
}

/// What the playground posts: one operation's input and options.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct PlaygroundPost {
    /// The input: a composition, a Bundle or a resource.
    pub input: String,
    /// The template id, when one is given.
    pub template: String,
    /// The patient reference `$tofhir` takes, when one is given.
    pub patient: String,
    /// The composition format `$toopenehr` answers in.
    pub format: String,
    /// The resource type `$validate` checks against.
    pub resource_type: String,
}

/// What the bridge answered a run of the playground.
#[derive(Debug, Clone)]
pub enum PlaygroundAnswer {
    /// The input cannot be sent as it stands, for this reason.
    Invalid(crate::locale::messages::Msg),
    /// The bridge answered with this document.
    Answered(Loaded<RunAnswer>),
}

/// A document the bridge answered a run with, and the issues of the
/// `OperationOutcome` it carried beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct RunAnswer {
    /// The document, as JSON.
    pub document: serde_json::Value,
    /// The words of each issue the answer carried beside the document.
    pub issues: Vec<String>,
}

impl From<serde_json::Value> for RunAnswer {
    fn from(document: serde_json::Value) -> Self {
        Self {
            document,
            issues: Vec::new(),
        }
    }
}

/// A run of the playground and what the bridge answered it.
#[derive(Debug, Clone)]
pub struct Ran {
    /// What was posted.
    pub post: PlaygroundPost,
    /// What the bridge answered.
    pub answer: PlaygroundAnswer,
}

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
fn token_of(
    state: &ConsoleState,
    parts: &http::request::Parts,
) -> Option<crate::bridge::AccessToken> {
    let id = state
        .sessions()
        .cookie_in(&parts.headers, crate::session::COOKIE)?;
    // NOTE: no specification governs this: our own design; a store that
    // cannot be read admits no post, and the page drawn after says why.
    let token = state.sessions().access_token(&id).ok().flatten()?;
    Some(crate::bridge::AccessToken::new(token))
}

/// Serves `POST /mappings`: reloads the set and draws the Mapping
/// set with the reload's outcome.
pub async fn reload(Extension(state): Extension<ConsoleState>, request: Request) -> Response {
    let (parts, _body) = request.into_parts();
    let Some(token) = token_of(&state, &parts) else {
        return back_to(&parts);
    };
    let answer =
        loaded(state.bridge().reload_mappings(&token).await).map(|outcome| match outcome {
            Outcome::Shown(answered) => Outcome::Shown(answered.report),
            Outcome::Refused(refusal) => Outcome::Refused(refusal),
        });
    let reloaded = Reloaded(answer);
    crate::server::render(state, parts, move || {
        leptos::context::provide_context(reloaded.clone());
    })
    .await
}

/// Serves `POST /mappings/edit/{path}` and `POST /mappings/new`: checks the
/// draft, asks to confirm a save, saves it, or steps back, and draws the
/// editor with the answer.
pub async fn edit(Extension(state): Extension<ConsoleState>, request: Request) -> Response {
    let (parts, mut post) = match form::<EditorPost>(request).await {
        Ok(read) => read,
        Err(answer) => return *answer,
    };
    // NOTE: HTML Living Standard §4.10.11: a text area submits its value with every
    // line break as CRLF, so the draft is read back with the LF the editor writes.
    post.draft = post.draft.replace("\r\n", "\n");
    let Some(token) = token_of(&state, &parts) else {
        return back_to(&parts);
    };
    if post.decision == "discard" {
        let back = editor_path(parts.uri.path(), &EditorPost::default()).map_or_else(
            || String::from(crate::app::MAPPINGS),
            |path| crate::pages::parts::file_href(&path),
        );
        return Redirect::to(&back).into_response();
    }
    let path = editor_path(parts.uri.path(), &post);
    let answer = crate::pages::editor::answer(&state, &token, path.as_deref(), &post).await;
    let edited = Edited { post, answer };
    crate::server::render(state, parts, move || {
        leptos::context::provide_context(edited.clone());
    })
    .await
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
