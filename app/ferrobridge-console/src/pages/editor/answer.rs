// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the bridge answers a post of the editor.
//!
//! The reason and the path are checked, then the check, the confirmation or
//! the save is sent. The server runs it for the editor's post and its server
//! function alike.

use crate::actions::{EditorAnswer, EditorPost, Field};
use crate::bridge::AccessToken;
use crate::bridge::editing::{MAX_REASON, Precondition};
use crate::locale::messages::Msg;
use crate::model::loaded;
use crate::server::ConsoleState;

/// Why `reason` cannot go with a save, or `None` when it can: one line of
/// printable ASCII, at most [`MAX_REASON`] characters.
fn reason_fault(reason: &str) -> Option<Msg> {
    let reason = reason.trim();
    if reason.is_empty() {
        Some(Msg::ReasonMissing)
    } else if reason.chars().count() > MAX_REASON {
        Some(Msg::ReasonTooLong)
    } else if !reason.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        Some(Msg::ReasonCharacters)
    } else {
        None
    }
}

/// Why `path` cannot name a mapping file, or `None` when it can: plain
/// segments below `fhirconnect/` or `omocl/`, ending in `.yml`.
fn path_fault(path: &str) -> Option<Msg> {
    let segments = path.split('/').collect::<Vec<_>>();
    if path.is_empty() {
        Some(Msg::PathMissing)
    } else if segments
        .iter()
        .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        Some(Msg::PathSteps)
    } else if !matches!(segments.first(), Some(&("fhirconnect" | "omocl"))) || segments.len() < 2 {
        Some(Msg::PathRoot)
    } else if !std::path::Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "yml" || extension == "yaml")
    {
        Some(Msg::PathExtension)
    } else {
        None
    }
}

/// What the bridge answers the post `post` of the editor of `path`, as the
/// operator whose access token is `token`.
pub async fn answer(
    state: &ConsoleState,
    token: &AccessToken,
    path: Option<&str>,
    post: &EditorPost,
) -> EditorAnswer {
    let named = post.path.trim();
    let Some(path) = path else {
        return EditorAnswer::Invalid {
            field: Field::Path,
            said: Msg::PathMissing,
        };
    };
    if !named.is_empty()
        && let Some(said) = path_fault(named)
    {
        return EditorAnswer::Invalid {
            field: Field::Path,
            said,
        };
    }
    match post.decision.as_str() {
        "check" => EditorAnswer::Checked(loaded(
            state
                .bridge()
                .check_mapping(path, post.draft.clone().into_bytes(), token)
                .await,
        )),
        "save" => match reason_fault(&post.reason) {
            Some(said) => EditorAnswer::Invalid {
                field: Field::Reason,
                said,
            },
            None => EditorAnswer::Confirm,
        },
        "confirm" => {
            if let Some(said) = reason_fault(&post.reason) {
                return EditorAnswer::Invalid {
                    field: Field::Reason,
                    said,
                };
            }
            // NOTE: RFC 9110 §13.1.2: a file named anew must not exist yet, and one
            // edited in place must still hold the digest it was opened at.
            let precondition = if post.base.is_empty() || !named.is_empty() {
                Precondition::Absent
            } else {
                Precondition::Match(post.base.clone())
            };
            EditorAnswer::Saved(loaded(
                state
                    .bridge()
                    .save_mapping(
                        path,
                        post.draft.clone().into_bytes(),
                        &precondition,
                        &post.reason,
                        token,
                    )
                    .await,
            ))
        }
        _ => EditorAnswer::Back,
    }
}

#[cfg(test)]
mod tests {
    use super::{path_fault, reason_fault};

    #[test]
    fn a_reason_is_one_line_of_plain_characters_and_not_too_long() {
        assert_eq!(None, reason_fault("Follow version 1.3.0 of the profile"));
        assert!(reason_fault("  ").is_some());
        assert!(reason_fault("tick \u{2713}").is_some());
        assert!(reason_fault(&"a".repeat(201)).is_some());
    }

    #[test]
    fn a_new_path_is_plain_segments_below_a_mapping_root() {
        assert_eq!(None, path_fault("fhirconnect/models/a.model.yml"));
        assert_eq!(None, path_fault("omocl/drug.yml"));
        assert!(path_fault("fhirconnect/../a.yml").is_some());
        assert!(path_fault("templates/a.yml").is_some());
        assert!(path_fault("fhirconnect/a.json").is_some());
        assert!(path_fault("").is_some());
    }
}
