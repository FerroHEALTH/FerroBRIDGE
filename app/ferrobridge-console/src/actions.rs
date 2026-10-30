// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the operator posts and what the bridge answered: Reload the set,
//! the editor's check and save, and the playground's runs.
//!
//! Each action is a plain form that posts to the page it is on, and the
//! page's route draws the page again with the bridge's answer and the
//! posted fields, answering `200` (FerroUI `docs/consoles.md`, "An action
//! that answers on its page"). Nothing the operator entered or the bridge
//! answered reaches a URL, the history or a store. No specification governs
//! the console: our own design.

use ferrobridge_operator_api::operator::LoadReport;

use crate::locale::messages::Msg;
use crate::model::{Answered, Loaded};

/// The outcome of the reload the operator just asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Reloaded(pub Loaded<LoadReport>);

/// What the editor posts: the draft and what it asks of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// The post the editor's form body `body` carried.
    #[must_use]
    pub fn from_form(body: &str) -> Self {
        let field = |name: &str| field(body, name);
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The reason a save gives.
    Reason,
    /// The path a new file saves under.
    Path,
}

/// What the bridge answered a post of the editor.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct Edited {
    /// What was posted.
    pub post: EditorPost,
    /// What the bridge answered.
    pub answer: EditorAnswer,
}

/// What the playground posts: one operation's input and options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// The post the playground's form body `body` carried.
    #[must_use]
    pub fn from_form(body: &str) -> Self {
        let field = |name: &str| field(body, name);
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
#[derive(Debug, Clone, PartialEq)]
pub enum PlaygroundAnswer {
    /// The input cannot be sent as it stands, for this reason.
    Invalid(Msg),
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
#[derive(Debug, Clone, PartialEq)]
pub struct Ran {
    /// What was posted.
    pub post: PlaygroundPost,
    /// What the bridge answered.
    pub answer: PlaygroundAnswer,
}

/// The last value of the field `name` in the form body `body`
/// (`application/x-www-form-urlencoded`), empty when it carries none.
#[must_use]
pub fn field(body: &str, name: &str) -> String {
    url::form_urlencoded::parse(body.as_bytes())
        .filter(|(key, _)| key == name)
        .last()
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{EditorPost, field};

    #[test]
    fn a_form_body_reads_by_field_and_the_last_value_wins() {
        let body = "draft=a%0D%0Ab&decision=check&decision=save&reason=why+not";
        assert_eq!("a\r\nb", field(body, "draft"));
        assert_eq!("save", field(body, "decision"));
        assert_eq!("", field(body, "path"));
        let post = EditorPost::from_form(body);
        assert_eq!("why not", post.reason);
        assert_eq!("save", post.decision);
    }
}
