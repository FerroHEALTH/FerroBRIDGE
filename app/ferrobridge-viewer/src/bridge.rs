// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one module that calls the bridge: its public HTTP surface, server to
//! server, read into the workspace's own typed bodies.
//!
//! The console compiles and runs no mapping itself, so every check,
//! transform and save goes through a call here. An answer the bridge refused
//! is a [`BridgeError::Refused`] carrying its status and body, never an
//! empty value. No specification governs the console: our own design.

use http::StatusCode;
use http::header::ACCEPT;
use url::Url;

use fhir_types::codec::{Json as _, Path, Value};
use fhir_types::r4::capability_statement::CapabilityStatement;

use crate::config::settings::BridgeSettings;

/// The FHIR JSON media type (FHIR R4 REST API, `http.html`, content types
/// and encodings).
pub const FHIR_JSON: &str = "application/fhir+json";

/// The largest body the console reads from an answer the bridge accepted.
pub const BODY_LIMIT: usize = 4 * 1024 * 1024;

/// The most of a refused answer's body a [`BridgeError::Refused`] keeps.
pub const REFUSAL_LIMIT: usize = 16 * 1024;

/// A call to the bridge that did not produce its typed answer.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// The client could not be built, or the request did not complete: no
    /// connection, a broken exchange, or no answer within the timeout.
    #[error("the bridge could not be reached")]
    Transport {
        /// What failed.
        #[source]
        source: reqwest::Error,
    },
    /// A route did not join onto the configured base URL.
    #[error("`{route}` does not join onto the bridge's base URL")]
    Route {
        /// The route.
        route: &'static str,
        /// What joining it failed with.
        #[source]
        source: url::ParseError,
    },
    /// The bridge answered with a status other than success.
    #[error("the bridge answered {status}")]
    Refused {
        /// The bridge's status.
        status: StatusCode,
        /// The start of the bridge's body, at most [`REFUSAL_LIMIT`] bytes,
        /// as text.
        body: String,
    },
    /// The accepted answer's body is larger than [`BODY_LIMIT`].
    #[error("the bridge's answer is over {limit} bytes")]
    TooLarge {
        /// The limit.
        limit: usize,
    },
    /// The accepted answer's body is not JSON.
    #[error("the bridge's answer is not JSON")]
    Payload {
        /// What parsing it failed with.
        #[source]
        source: serde_json::Error,
    },
    /// The accepted answer's body is JSON but no object.
    #[error("the bridge's answer is not a JSON object")]
    NotAnObject,
    /// The accepted answer's body does not decode as the resource asked for.
    #[error("the bridge's answer is not a {expected}")]
    Decode {
        /// The resource type asked for.
        expected: &'static str,
        /// Where and why it was refused.
        #[source]
        source: Box<fhir_types::codec::DecodeError>,
    },
}

/// The client of one bridge.
#[derive(Debug, Clone)]
pub struct Bridge {
    client: reqwest::Client,
    base: Url,
}

impl Bridge {
    /// Builds the client for the bridge `settings` names.
    ///
    /// # Errors
    /// Returns [`BridgeError::Transport`] when the HTTP client cannot be
    /// built.
    pub fn new(settings: &BridgeSettings) -> Result<Self, BridgeError> {
        // NOTE: no specification governs this: our own design; the bridge is
        // called server to server and only its own answer counts.
        let client = reqwest::Client::builder()
            .timeout(settings.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|source| BridgeError::Transport { source })?;
        Ok(Self {
            client,
            base: settings.base.clone(),
        })
    }

    /// The root of the bridge's HTTP surface.
    #[must_use]
    pub fn base(&self) -> &Url {
        &self.base
    }

    /// Reads the bridge's `CapabilityStatement` from `GET {base}fhir/metadata`,
    /// the capabilities interaction of the FHIR R4 REST API (`http.html`).
    ///
    /// # Errors
    /// Returns [`BridgeError::Route`] when the route does not join onto the
    /// base, [`BridgeError::Transport`] when the call does not complete,
    /// [`BridgeError::Refused`] for a status other than success, and
    /// [`BridgeError::TooLarge`], [`BridgeError::Payload`],
    /// [`BridgeError::NotAnObject`] and [`BridgeError::Decode`] for a body
    /// that is no R4 `CapabilityStatement`.
    pub async fn capability_statement(&self) -> Result<CapabilityStatement, BridgeError> {
        // TODO(#600): send the signed-in operator's access token.
        let object = self.fhir_object("fhir/metadata").await?;
        CapabilityStatement::from_json(&object, &mut Path::root("CapabilityStatement")).map_err(
            |source| BridgeError::Decode {
                expected: "CapabilityStatement",
                source: Box::new(source),
            },
        )
    }

    /// Sends `GET {base}{route}` for FHIR JSON and reads the JSON object
    /// the bridge answers.
    async fn fhir_object(
        &self,
        route: &'static str,
    ) -> Result<fhir_types::codec::Object, BridgeError> {
        let url = self.url(route)?;
        let response = self
            .client
            .get(url)
            .header(ACCEPT, FHIR_JSON)
            .send()
            .await
            .map_err(|source| BridgeError::Transport { source })?;
        let status = response.status();
        if !status.is_success() {
            let body = read(response, REFUSAL_LIMIT, Overflow::Truncate).await?;
            return Err(BridgeError::Refused {
                status,
                body: String::from_utf8_lossy(&body).into_owned(),
            });
        }
        let body = read(response, BODY_LIMIT, Overflow::Refuse).await?;
        let value: serde_json::Value =
            serde_json::from_slice(&body).map_err(|source| BridgeError::Payload { source })?;
        match Value::from_serde_json(value) {
            Value::Object(object) => Ok(object),
            _ => Err(BridgeError::NotAnObject),
        }
    }

    /// The URL of `route` under the base.
    fn url(&self, route: &'static str) -> Result<Url, BridgeError> {
        self.base
            .join(route)
            .map_err(|source| BridgeError::Route { route, source })
    }
}

/// What [`read`] does with a body over its limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overflow {
    /// Keeps the first bytes up to the limit.
    Truncate,
    /// Refuses the body with [`BridgeError::TooLarge`].
    Refuse,
}

/// Reads `response`'s body up to `limit` bytes.
async fn read(
    mut response: reqwest::Response,
    limit: usize,
    overflow: Overflow,
) -> Result<Vec<u8>, BridgeError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|source| BridgeError::Transport { source })?
    {
        let room = limit.saturating_sub(body.len());
        if chunk.len() > room {
            if overflow == Overflow::Refuse {
                return Err(BridgeError::TooLarge { limit });
            }
            if let Some(head) = chunk.get(..room) {
                body.extend_from_slice(head);
            }
            break;
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
