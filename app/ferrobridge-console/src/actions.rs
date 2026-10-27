// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the operator posts and what the bridge answered: Reload the set,
//! the editor's check and save, and the playground's runs.
//!
//! Each action is a server function behind FerroUI's `<ActionForm>` in its
//! answer mode. Once the page has hydrated, the form calls the function and
//! the page draws its answer in place. With no WebAssembly the form posts to
//! the function, and `ferrohealth_ui::widget::action::draw_answers` draws
//! the page the form was on with the answer and the posted fields. Either
//! way nothing the operator entered or the bridge answered reaches a URL,
//! the history or a store. No specification governs the console: our own
//! design.
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        clippy::unused_async_trait_impl,
        reason = "the server function macro writes the browser half's call as an async trait method; only that target has it"
    )
)]

use ferrobridge_operator_api::operator::LoadReport;
use ferrohealth_ui::widget::action::Posted;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::locale::messages::Msg;
use crate::model::{Answered, Loaded, ViewError};

/// The outcome of the reload the operator just asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reloaded(pub Loaded<LoadReport>);

/// What the editor posts: the draft and what it asks of it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// What the post asks: `check`, `save`, `confirm`, `back` or `discard`.
    pub decision: String,
}

impl EditorPost {
    /// The post a plain post of the editor's form carried, as `posted`
    /// holds it.
    #[must_use]
    pub fn from_posted(posted: &Posted) -> Self {
        let field = |name: &str| posted.field(name).unwrap_or_default().to_owned();
        Self {
            draft: field("draft"),
            base: field("base"),
            path: field("path"),
            reason: field("reason"),
            decision: field("decision"),
        }
    }
}

/// The field of the editor's form a post is refused for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Field {
    /// The reason a save gives.
    Reason,
    /// The path a new file saves under.
    Path,
}

/// What the bridge answered a post of the editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        /// The field at fault.
        field: Field,
        /// Why.
        said: Msg,
    },
}

/// A post of the editor and what the bridge answered it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edited {
    /// What was posted.
    pub post: EditorPost,
    /// What the bridge answered.
    pub answer: EditorAnswer,
}

/// What the playground posts: one operation's input and options.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

impl PlaygroundPost {
    /// The post a plain post of the playground's form carried, as `posted`
    /// holds it.
    #[must_use]
    pub fn from_posted(posted: &Posted) -> Self {
        let field = |name: &str| posted.field(name).unwrap_or_default().to_owned();
        Self {
            input: field("input"),
            template: field("template"),
            patient: field("patient"),
            format: field("format"),
            resource_type: field("resource_type"),
        }
    }
}

/// What the bridge answered a run of the playground.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlaygroundAnswer {
    /// The input cannot be sent as it stands, for this reason.
    Invalid(Msg),
    /// The bridge answered with this document.
    Answered(Loaded<RunAnswer>),
}

/// A document the bridge answered a run with, and the issues of the
/// `OperationOutcome` it carried beside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ran {
    /// What was posted.
    pub post: PlaygroundPost,
    /// What the bridge answered.
    pub answer: PlaygroundAnswer,
}

/// Reloads the set.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, and
/// [`ViewError::Unavailable`] when the console's server does not answer.
#[server(endpoint = "reload")]
pub async fn reload_set() -> Result<Reloaded, ViewError> {
    let (state, token) = crate::session::request::signed_in()?;
    Ok(crate::posts::reloaded(&state, &token).await)
}

/// Checks the draft, asks to confirm a save, saves it, or steps back, for
/// the editor at `address`. Discard answers `303 See Other` to the file the
/// draft was opened from; only a plain post sends it, as the page discards
/// in place once it has hydrated.
///
/// # Errors
/// As [`reload_set`].
#[server(endpoint = "edit")]
pub async fn edit_draft(
    /// The editor's own address.
    address: String,
    /// What the editor posts.
    #[server(flatten)]
    post: EditorPost,
) -> Result<Edited, ViewError> {
    let (state, token) = crate::session::request::signed_in()?;
    if post.decision == "discard" {
        crate::posts::leave_for(&crate::posts::discarded(&address))?;
        return Ok(Edited {
            post,
            answer: EditorAnswer::Back,
        });
    }
    Ok(crate::posts::edited(&state, &token, &address, post).await)
}

/// Runs the operation of the playground's tab at `address` over what the
/// operator entered.
///
/// # Errors
/// As [`reload_set`].
#[server(endpoint = "run")]
pub async fn run_operation(
    /// The address of the playground's tab.
    address: String,
    /// What the playground posts.
    #[server(flatten)]
    post: PlaygroundPost,
) -> Result<Ran, ViewError> {
    let (state, token) = crate::session::request::signed_in()?;
    let answer = crate::pages::playground::answer::answer(&state, &token, &address, &post).await;
    Ok(Ran { post, answer })
}
