// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the posts of `crate::actions` ask the bridge.
//!
//! Reload the set, the editor's post, where a discarded draft goes back to,
//! the replay of a parked change-feed event, and the apply of a revision. No
//! specification governs the console: our own design.

use crate::actions::{
    ApplyAnswer, ApplyPost, Edited, EditorPost, ImportAnswer, ImportPost, Reloaded, ReplayAnswer,
    ReplayPost,
};
use crate::bridge::AccessToken;
use crate::locale::messages::Msg;
use crate::model::{Outcome, loaded};
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

/// The answer to the replay `post`, as the operator whose token is `token`:
/// the bridge is asked only for a consumer and a number it can name.
pub async fn replayed(
    state: &ConsoleState,
    token: &AccessToken,
    post: &ReplayPost,
) -> ReplayAnswer {
    let Ok(id) = post.id.trim().parse::<u64>() else {
        return ReplayAnswer::Invalid;
    };
    if post.consumer.is_empty() {
        return ReplayAnswer::Invalid;
    }
    ReplayAnswer::Answered(loaded(
        state
            .bridge()
            .replay_parked(&post.consumer, id, token)
            .await,
    ))
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

/// The answer to the apply `post` of the revision `id`, as the operator
/// whose token is `token`: the bridge is asked only with a reason it takes.
pub async fn applied(
    state: &ConsoleState,
    token: &AccessToken,
    id: &str,
    post: &ApplyPost,
) -> ApplyAnswer {
    if let Some(fault) = crate::pages::editor::answer::reason_fault(&post.reason) {
        return ApplyAnswer::Invalid(fault);
    }
    ApplyAnswer::Answered(loaded(
        state
            .bridge()
            .apply_revision(id, post.running.trim(), &post.reason, token)
            .await,
    ))
}

/// The import page's post read from its multipart body `raw` in
/// `content_type`, or `None` for a body that is no multipart form.
pub async fn import_form(raw: &[u8], content_type: Option<&str>) -> Option<ImportPost> {
    use axum::extract::FromRequest as _;
    let request = http::Request::builder()
        .header(http::header::CONTENT_TYPE, content_type?)
        .body(axum::body::Body::from(raw.to_vec()))
        .ok()?;
    // NOTE: no specification governs this: our own design; a body that is no
    // multipart form carries no archive, which the page asks for again.
    let mut form = axum::extract::Multipart::from_request(request, &())
        .await
        .ok()?;
    let mut post = ImportPost::default();
    while let Some(field) = form.next_field().await.ok()? {
        match field.name() {
            Some("archive") => post.archive = field.bytes().await.ok()?.to_vec(),
            Some("reason") => post.reason = field.text().await.ok()?,
            Some("decision") => post.decision = field.text().await.ok()?,
            _ => {}
        }
    }
    Some(post)
}

/// The answer to the import `post`, as the operator whose token is
/// `token`: the bridge is asked only with an archive and a reason it takes.
pub async fn imported(
    state: &ConsoleState,
    token: &AccessToken,
    post: &ImportPost,
) -> ImportAnswer {
    if post.archive.is_empty() {
        return ImportAnswer::Invalid(Msg::ImportNoArchive);
    }
    if let Some(fault) = crate::pages::editor::answer::reason_fault(&post.reason) {
        return ImportAnswer::Invalid(fault);
    }
    ImportAnswer::Answered(loaded(
        state
            .bridge()
            .import_revision(post.archive.clone(), &post.reason, token)
            .await,
    ))
}
