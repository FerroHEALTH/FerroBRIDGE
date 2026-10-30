// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a page reads from the bridge, split into the bridge's answer and the
//! console's own faults.
//!
//! An answer is an [`Outcome`]: the document, or the bridge's refusal with
//! its status. Only the console's own faults (no live session, a bridge it
//! cannot reach, a body it cannot read) are a [`ViewError`], so a page
//! branches on two distinct types. Both cross to the browser as JSON, the
//! answer of a server function, so a page drawn in the browser branches as
//! the server's does. No specification governs the console: our own design.

use ferrobridge_operator_api::operator::LoadReport;
use ferrobridge_operator_api::problem::Problem;
use fhir_types::codec::{Json as _, Path, Value};
use fhir_types::r4::operation_outcome::OperationOutcome;
use http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::bridge::BridgeError;
use crate::locale::messages::Msg;
use ferrohealth_ui::i18n::Locale;

use crate::locale::{tr, tr_with};

/// What the bridge answered when the console did its part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Outcome<T> {
    /// The document.
    Shown(T),
    /// The bridge refused the request.
    Refused(Refusal),
}

impl<T> Outcome<T> {
    /// Returns the outcome with its document turned by `turn`, a refusal as
    /// it is.
    pub fn map<U>(self, turn: impl FnOnce(T) -> U) -> Outcome<U> {
        match self {
            Self::Shown(document) => Outcome::Shown(turn(document)),
            Self::Refused(refusal) => Outcome::Refused(refusal),
        }
    }
}

/// An expected refusal of the bridge's, with its status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    /// The bridge refused the request.
    Bridge {
        /// The status the bridge answered with.
        #[serde(with = "status_code")]
        status: StatusCode,
        /// The diagnostics of the `OperationOutcome` or the title and detail
        /// of the problem it answered with, in order; empty when its body
        /// carried none.
        diagnostics: Vec<String>,
    },
    /// The bridge did not accept the operator's access token (`401`), so the
    /// operator signs in again.
    NotAuthenticated,
    /// The operator's input cannot be sent as it stands; the reason names
    /// the field and never quotes what was entered.
    Invalid {
        /// What is wrong, by field.
        reason: Msg,
    },
}

impl Refusal {
    /// The words of the refusal in `locale`: its status and the
    /// diagnostics the bridge carried, in order.
    #[must_use]
    pub fn text(&self, locale: &Locale) -> String {
        match self {
            Self::Bridge {
                status,
                diagnostics,
            } => {
                let mut text = tr_with(
                    locale,
                    Msg::BridgeRefusedStatus,
                    &[("status", status.as_u16().to_string().into())],
                );
                for said in diagnostics {
                    text.push(' ');
                    text.push_str(said);
                }
                text
            }
            Self::NotAuthenticated => tr(locale, Msg::BridgeRefusedSignIn),
            Self::Invalid { reason } => tr(locale, *reason),
        }
    }

    /// The status the bridge answered with, when it answered.
    #[must_use]
    pub const fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Bridge { status, .. } => Some(*status),
            Self::NotAuthenticated => Some(StatusCode::UNAUTHORIZED),
            Self::Invalid { .. } => None,
        }
    }

    /// Whether the bridge answered `404`, as it does for a surface it does
    /// not serve, such as the operator API while `[operator]` is off.
    #[must_use]
    pub fn not_served(&self) -> bool {
        self.status() == Some(StatusCode::NOT_FOUND)
    }
}

/// A fault of the console: its session, its reach to the bridge, or a body
/// it cannot read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum ViewError {
    /// The request carries no live signed-in session.
    #[error("sign in to see this view")]
    SignedOut,
    /// The bridge answered with a body this console cannot read.
    #[error("the bridge answered with a body this console cannot read")]
    Unreadable,
    /// The bridge gave no answer at all.
    #[error("the bridge could not be reached")]
    Unreachable,
    /// The console could not serve the view.
    #[error("the console could not serve this view")]
    Unavailable,
}

impl ViewError {
    /// The words of the fault in `locale`; its `Display` is the log's.
    #[must_use]
    pub fn text(&self, locale: &Locale) -> String {
        tr(
            locale,
            match self {
                Self::SignedOut => Msg::ViewSignedOut,
                Self::Unreadable => Msg::ViewUnreadable,
                Self::Unreachable => Msg::ViewUnreachable,
                Self::Unavailable => Msg::ViewUnavailable,
            },
        )
    }
}

/// What a page loads: the bridge's answer, or the console's fault.
pub type Loaded<T> = Result<Outcome<T>, ViewError>;

/// The serde form of a status: its number.
pub(crate) mod status_code {
    use http::StatusCode;
    use serde::{Deserialize as _, Deserializer, Serializer};

    /// Writes `status` as its number.
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's `with` calls the serializer with a reference to the field"
    )]
    pub(super) fn serialize<S: Serializer>(status: &StatusCode, out: S) -> Result<S::Ok, S::Error> {
        out.serialize_u16(status.as_u16())
    }

    /// Reads a status from its number, refusing one outside 100 to 999.
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<StatusCode, D::Error> {
        let number = u16::deserialize(input)?;
        StatusCode::from_u16(number).map_err(serde::de::Error::custom)
    }
}

/// What the bridge answered a check, a save or a reload with: the status,
/// the report of the set, and after a save the new file's digest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answered {
    /// `200` for a set that compiled, `422` for one that did not.
    #[serde(with = "status_code")]
    pub status: StatusCode,
    /// The report of the set: whether it compiled, its counts and every
    /// diagnostic.
    pub report: LoadReport,
    /// The digest the new file's `ETag` names, after a save.
    pub etag: Option<String>,
}

/// One mapping file as a page draws it: its text, and the digest of the
/// bytes the bridge served.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenedFile {
    /// The file's bytes as UTF-8 text, or `None` for bytes that are not.
    pub text: Option<String>,
    /// The digest its `ETag` names, the quotes removed, when it sent one.
    pub etag: Option<String>,
}

/// What the server knows of the request's session, which decides the page
/// drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// A live signed-in session: the operator's pages and the sign-out.
    SignedIn,
    /// No live session: the sign-in, and no operator page.
    SignedOut,
    /// The session store cannot be read, so the console cannot tell: the
    /// page says so, and offers neither the sign-in nor the operator's pages.
    Unreadable,
}

/// The refusal `error` carries, or the console's fault it is.
///
/// A refusal is logged with its status alone, a fault with its cause;
/// neither line carries a body or a token.
///
/// # Errors
/// Returns [`ViewError::Unreachable`] for a bridge that gave no answer,
/// [`ViewError::Unreadable`] for an answer this console cannot read, and
/// [`ViewError::Unavailable`] for a request the console could not send.
pub fn refusal(error: &BridgeError) -> Result<Refusal, ViewError> {
    match error {
        BridgeError::Refused { status, body } => {
            tracing::warn!(%status, "the bridge refused a view");
            if *status == StatusCode::UNAUTHORIZED {
                return Ok(Refusal::NotAuthenticated);
            }
            Ok(Refusal::Bridge {
                status: *status,
                diagnostics: diagnostics(body),
            })
        }
        BridgeError::Transport { .. } => {
            tracing::error!(error = %crate::command::chain(error), "the bridge could not be reached");
            Err(ViewError::Unreachable)
        }
        BridgeError::TooLarge { .. }
        | BridgeError::Payload { .. }
        | BridgeError::NotAnObject
        | BridgeError::Document { .. }
        | BridgeError::Decode { .. } => {
            tracing::error!(error = %crate::command::chain(error), "the bridge answered a body this console cannot read");
            Err(ViewError::Unreadable)
        }
        BridgeError::Path => Ok(Refusal::Invalid {
            reason: Msg::InvalidPath,
        }),
        BridgeError::ResourceType => Ok(Refusal::Invalid {
            reason: Msg::InvalidResourceType,
        }),
        BridgeError::Reason => Ok(Refusal::Invalid {
            reason: Msg::InvalidReason,
        }),
        BridgeError::Route { .. }
        | BridgeError::Token
        | BridgeError::Encode { .. }
        | BridgeError::Number { .. } => {
            tracing::error!(error = %crate::command::chain(error), "a request to the bridge could not be sent");
            Err(ViewError::Unavailable)
        }
    }
}

/// The answer `result` holds as a page loads it: the document, the bridge's
/// refusal, or the console's fault.
///
/// # Errors
/// As [`refusal`].
pub fn loaded<T>(result: Result<T, BridgeError>) -> Loaded<T> {
    match result {
        Ok(document) => Ok(Outcome::Shown(document)),
        Err(error) => refusal(&error).map(Outcome::Refused),
    }
}

/// The words of a refusal `body`: each issue of an R4 `OperationOutcome`, in
/// order, its `details.text` and its `diagnostics` when present (FHIR R4
/// `operationoutcome.html`, `OperationOutcome.issue`), or the `title` and
/// the `detail` of an RFC 9457 problem (§3.1.3, §3.1.4), which the bridge
/// answers outside its FHIR routes.
fn diagnostics(body: &str) -> Vec<String> {
    // NOTE: no specification governs this: our own design; a body that is
    // neither document carries no diagnostics, and its status stands.
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    if value.get("resourceType").is_none() {
        return serde_json::from_value::<Problem>(value)
            .map(|problem| problem.title.into_iter().chain(problem.detail).collect())
            .unwrap_or_default();
    }
    let Value::Object(object) = Value::from_serde_json(value) else {
        return Vec::new();
    };
    OperationOutcome::from_json(&object, &mut Path::root("OperationOutcome"))
        .map(|outcome| {
            outcome
                .issue
                .into_iter()
                .flat_map(|issue| {
                    let details = issue
                        .details
                        .and_then(|details| details.text)
                        .and_then(|text| text.value);
                    let diagnostics = issue.diagnostics.and_then(|text| text.value);
                    details.into_iter().chain(diagnostics)
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use super::{Refusal, ViewError, refusal};
    use crate::bridge::BridgeError;

    #[test]
    fn a_401_asks_for_a_new_sign_in_and_another_refusal_keeps_its_status_and_diagnostics() {
        let unauthorized = BridgeError::Refused {
            status: StatusCode::UNAUTHORIZED,
            body: String::new(),
        };
        assert_eq!(Ok(Refusal::NotAuthenticated), refusal(&unauthorized));
        let forbidden = BridgeError::Refused {
            status: StatusCode::FORBIDDEN,
            body: String::from(
                r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"forbidden","diagnostics":"the operator scope is missing"}]}"#,
            ),
        };
        assert_eq!(
            Ok(Refusal::Bridge {
                status: StatusCode::FORBIDDEN,
                diagnostics: vec![String::from("the operator scope is missing")],
            }),
            refusal(&forbidden)
        );
        let detailed = BridgeError::Refused {
            status: StatusCode::SERVICE_UNAVAILABLE,
            body: String::from(
                r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"not-supported","details":{"text":"the operations are not served"},"diagnostics":"no mapping set"}]}"#,
            ),
        };
        assert_eq!(
            Ok(Refusal::Bridge {
                status: StatusCode::SERVICE_UNAVAILABLE,
                diagnostics: vec![
                    String::from("the operations are not served"),
                    String::from("no mapping set"),
                ],
            }),
            refusal(&detailed),
            "an issue's details.text is its words too"
        );
        let problem = BridgeError::Refused {
            status: StatusCode::CONFLICT,
            body: String::from(
                r#"{"type":"https://ferrobridge.eu/docs/operate/problems.html#provisioned","title":"A provisioned file holds this path","status":409,"detail":"the console never writes one"}"#,
            ),
        };
        assert_eq!(
            Ok(Refusal::Bridge {
                status: StatusCode::CONFLICT,
                diagnostics: vec![
                    String::from("A provisioned file holds this path"),
                    String::from("the console never writes one"),
                ],
            }),
            refusal(&problem),
            "an RFC 9457 problem's title and detail are its words"
        );
        let plain = BridgeError::Refused {
            status: StatusCode::SERVICE_UNAVAILABLE,
            body: String::from("down"),
        };
        assert_eq!(
            Ok(Refusal::Bridge {
                status: StatusCode::SERVICE_UNAVAILABLE,
                diagnostics: Vec::new(),
            }),
            refusal(&plain)
        );
        assert_eq!(Err(ViewError::Unavailable), refusal(&BridgeError::Token));
        assert_eq!(
            Err(ViewError::Unreadable),
            refusal(&BridgeError::NotAnObject)
        );
    }

    #[test]
    fn a_refusal_and_a_fault_draw_their_english_words_from_the_catalog() {
        let english = ferrohealth_ui::i18n::Locale::english();
        let refused = Refusal::Bridge {
            status: StatusCode::SERVICE_UNAVAILABLE,
            diagnostics: vec![String::from("synthetic diagnostic")],
        };
        assert_eq!(
            "The bridge refused the request (503). synthetic diagnostic",
            refused.text(&english)
        );
        assert_eq!(
            "The bridge did not accept your sign-in (401). Sign in again.",
            Refusal::NotAuthenticated.text(&english)
        );
        assert_eq!(
            "The bridge answered with a body this console cannot read.",
            ViewError::Unreadable.text(&english)
        );
        assert!(
            Refusal::Bridge {
                status: StatusCode::NOT_FOUND,
                diagnostics: Vec::new()
            }
            .not_served()
        );
    }
}
