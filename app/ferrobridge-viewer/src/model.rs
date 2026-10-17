// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The view models the server functions answer, which both halves share.
//!
//! An answer is an [`Outcome`]: the view, or the bridge's refusal with its
//! status. Only the console's own faults (no live session, a bridge it
//! cannot reach, a body it cannot read) are a [`ViewError`], so a caller
//! branches on two distinct types. Every integer is fixed-size, since the
//! browser half is 32-bit. No specification governs the console: our own
//! design.

use leptos::server_fn::codec::JsonEncoding;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
use serde::{Deserialize, Serialize};

/// What a server function answers when the console did its part.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome<T> {
    /// The view.
    Shown(T),
    /// The bridge refused the request.
    Refused(Refusal),
}

/// An expected refusal of the bridge's, with its status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    /// The bridge refused the request.
    Bridge {
        /// The status the bridge answered with.
        status: u16,
        /// The diagnostics of the `OperationOutcome` it answered with, in
        /// order; empty when its body carried none.
        diagnostics: Vec<String>,
    },
    /// The bridge did not accept the operator's access token (`401`), so the
    /// operator signs in again.
    NotAuthenticated,
    /// The operator's input cannot be sent as it stands; the reason names
    /// the field and never quotes what was entered.
    Invalid {
        /// What is wrong, by field.
        reason: String,
    },
}

/// A fault of the console: its session, its reach to the bridge, or a body
/// it cannot read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum ViewError {
    /// The request carries no live signed-in session.
    #[error("sign in to see this view")]
    SignedOut,
    /// The bridge answered with a body this console cannot read.
    #[error("the bridge answered {status} with a body this console cannot read")]
    Unreadable {
        /// The status the bridge answered with.
        status: u16,
    },
    /// The bridge gave no answer at all.
    #[error("the bridge could not be reached")]
    Unreachable,
    /// The console could not serve the view.
    #[error("the console could not serve this view")]
    Unavailable,
    /// The view could not be fetched from the console's server.
    #[error("the view could not be fetched: {source}")]
    Fetch {
        /// What the server function transport reported.
        #[source]
        source: ServerFnErrorErr,
    },
}

impl FromServerFnError for ViewError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        Self::Fetch { source: value }
    }
}

/// What the server knows of the request's session, which decides the page
/// drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// A live signed-in session: the operator's pages and the sign-out.
    SignedIn,
    /// No live session: the sign-in, and no operator page.
    SignedOut,
    /// The session store cannot be read, so the console cannot tell: the
    /// page says so, and offers neither the sign-in nor the operator's pages.
    Unreadable,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SignedIn => "in",
            Self::SignedOut => "out",
            Self::Unreadable => "unreadable",
        })
    }
}

impl std::str::FromStr for SessionState {
    type Err = UnknownSessionState;

    fn from_str(said: &str) -> Result<Self, Self::Err> {
        match said {
            "in" => Ok(Self::SignedIn),
            "out" => Ok(Self::SignedOut),
            "unreadable" => Ok(Self::Unreadable),
            _ => Err(UnknownSessionState),
        }
    }
}

/// A serialized [`SessionState`] the browser cannot read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the page names a session state the console does not know")]
pub struct UnknownSessionState;

/// What the bridge says of itself in its `CapabilityStatement`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeView {
    /// The software's name, `software.name`.
    pub software: Option<String>,
    /// The software's version, `software.version`.
    pub version: Option<String>,
    /// The FHIR version it serves, `fhirVersion`.
    pub fhir_version: Option<String>,
}

/// The server half: a failed bridge call split into the bridge's refusal or
/// the console's fault.
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    use fhir_types::codec::{Json as _, Path, Value};
    use fhir_types::r4::operation_outcome::OperationOutcome;
    use http::StatusCode;

    use crate::bridge::BridgeError;
    use crate::model::{Refusal, ViewError};

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
                    status: status.as_u16(),
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
            | BridgeError::NotAnArray
            | BridgeError::Decode { .. } => {
                tracing::error!(error = %crate::command::chain(error), "the bridge answered a body this console cannot read");
                Err(ViewError::Unreadable {
                    status: StatusCode::OK.as_u16(),
                })
            }
            BridgeError::Path => Ok(Refusal::Invalid {
                reason: String::from(
                    "the mapping file path is not plain names below a root, so it is not asked for",
                ),
            }),
            BridgeError::Route { .. } | BridgeError::Token => {
                tracing::error!(error = %crate::command::chain(error), "a request to the bridge could not be sent");
                Err(ViewError::Unavailable)
            }
        }
    }

    /// The diagnostics of the R4 `OperationOutcome` `body` holds, in order.
    fn diagnostics(body: &str) -> Vec<String> {
        // NOTE: no specification governs this: our own design; a refusal body
        // that is no OperationOutcome carries no diagnostics, and its status stands.
        let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
            return Vec::new();
        };
        let Value::Object(object) = Value::from_serde_json(value) else {
            return Vec::new();
        };
        OperationOutcome::from_json(&object, &mut Path::root("OperationOutcome"))
            .map(|outcome| {
                outcome
                    .issue
                    .into_iter()
                    .filter_map(|issue| issue.diagnostics.and_then(|text| text.value))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[cfg(test)]
    mod tests {
        use http::StatusCode;

        use super::refusal;
        use crate::bridge::BridgeError;
        use crate::model::{Refusal, ViewError};

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
                    status: 403,
                    diagnostics: vec![String::from("the operator scope is missing")],
                }),
                refusal(&forbidden)
            );
            let plain = BridgeError::Refused {
                status: StatusCode::SERVICE_UNAVAILABLE,
                body: String::from("down"),
            };
            assert_eq!(
                Ok(Refusal::Bridge {
                    status: 503,
                    diagnostics: Vec::new(),
                }),
                refusal(&plain)
            );
            assert_eq!(Err(ViewError::Unavailable), refusal(&BridgeError::Token));
            assert_eq!(
                Err(ViewError::Unreadable { status: 200 }),
                refusal(&BridgeError::NotAnObject)
            );
        }
    }
}
