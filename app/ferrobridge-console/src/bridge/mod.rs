// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one module that calls the bridge: its public HTTP surface, server to
//! server, read into the workspace's own typed bodies.
//!
//! The console compiles and runs no mapping itself, so every check,
//! transform and save goes through a call here. An answer the bridge refused
//! is a [`BridgeError::Refused`] carrying its status and body, never an
//! empty value. No specification governs the console: our own design.

use std::fmt;

use http::header::{ACCEPT, AUTHORIZATION};
use http::{HeaderValue, StatusCode};
use secrecy::{ExposeSecret as _, SecretString};
use url::Url;

use ferrobridge_operator_api::configuration::Configuration;
use ferrobridge_operator_api::health::{Info, Readiness};
use ferrobridge_operator_api::operator::{EngineState, Listed};
use fhir_types::codec::{Json as _, Path, Value};
use fhir_types::r4::capability_statement::CapabilityStatement;

use crate::config::settings::BridgeSettings;

pub mod editing;
pub mod operations;

/// The FHIR JSON media type (FHIR R4 REST API, `http.html`, content types
/// and encodings).
pub const FHIR_JSON: &str = "application/fhir+json";

/// The JSON media type of the bridge's own documents (RFC 8259 §11).
pub const JSON: &str = "application/json";

/// The YAML media type a mapping file is served with (RFC 9512).
pub const YAML: &str = "application/yaml";

/// One mapping file as the operator API serves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingFile {
    /// Its bytes, verbatim.
    pub bytes: Vec<u8>,
    /// The digest its `ETag` names, the quotes removed, when it sent one.
    pub etag: Option<String>,
}

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
    /// The operator's access token holds a character an HTTP header cannot
    /// carry (RFC 9110 §5.5), so it cannot be sent.
    #[error("the operator's access token cannot be sent as a header")]
    Token,
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
    /// The accepted answer's body is no health or operator document of the
    /// kind asked for (`ferrobridge_operator_api`).
    #[error("the bridge's answer is not its {expected}")]
    Document {
        /// The document asked for.
        expected: &'static str,
        /// What reading it failed with.
        #[source]
        source: serde_json::Error,
    },
    /// A mapping file path that is not plain segments below a root.
    #[error("the mapping file path is not plain names below a root")]
    Path,
    /// A resource type name that is not ASCII letters.
    #[error("the resource type is not a FHIR resource type name")]
    ResourceType,
    /// A save's reason that is empty, too long, or not printable ASCII on
    /// one line, which the bridge's reason header requires.
    #[error("the reason is not one line of at most 200 printable ASCII characters")]
    Reason,
    /// A request body the console built has no JSON form.
    #[error("the request could not be written as JSON")]
    Encode {
        /// What the encoder refused.
        #[source]
        source: Box<fhir_types::codec::EncodeError>,
    },
    /// A request body holds a number JSON cannot carry exactly.
    #[error("the request holds a number JSON cannot carry")]
    Number {
        /// What the conversion refused.
        #[source]
        source: Box<fhir_types::codec::ValueConversionError>,
    },
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

impl BridgeError {
    /// The bridge's status and the start of its body, when the bridge
    /// answered and refused.
    #[must_use]
    pub fn refusal(&self) -> Option<(StatusCode, &str)> {
        match self {
            Self::Refused { status, body } => Some((*status, body.as_str())),
            _ => None,
        }
    }
}

/// The signed-in operator's access token, which every call to the bridge
/// carries as a bearer token (RFC 6750 §2.1).
///
/// It never renders its value, so no `Debug` output or log line shows it.
#[derive(Clone)]
pub struct AccessToken(SecretString);

impl AccessToken {
    /// Wraps the access token `token`.
    #[must_use]
    pub fn new(token: SecretString) -> Self {
        Self(token)
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessToken(..)")
    }
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
    /// the capabilities interaction of the FHIR R4 REST API (`http.html`),
    /// as the operator whose access token is `token`.
    ///
    /// # Errors
    /// Returns [`BridgeError::Route`] when the route does not join onto the
    /// base, [`BridgeError::Transport`] when the call does not complete,
    /// [`BridgeError::Refused`] for a status other than success, and
    /// [`BridgeError::TooLarge`], [`BridgeError::Payload`],
    /// [`BridgeError::NotAnObject`] and [`BridgeError::Decode`] for a body
    /// that is no R4 `CapabilityStatement`.
    pub async fn capability_statement(
        &self,
        token: &AccessToken,
    ) -> Result<CapabilityStatement, BridgeError> {
        self.capability(token)
            .await
            .map(|(statement, _raw)| statement)
    }

    /// Reads the bridge's `CapabilityStatement` as
    /// [`Self::capability_statement`] does, with the JSON it arrived in.
    ///
    /// # Errors
    /// As [`Self::capability_statement`].
    pub async fn capability(
        &self,
        token: &AccessToken,
    ) -> Result<(CapabilityStatement, serde_json::Value), BridgeError> {
        let (_status, _headers, body) = self
            .fetch(
                self.url("fhir/metadata")?,
                FHIR_JSON,
                token,
                &[StatusCode::OK],
            )
            .await?;
        let raw: serde_json::Value =
            serde_json::from_slice(&body).map_err(|source| BridgeError::Payload { source })?;
        let Value::Object(object) = Value::from_serde_json(raw.clone()) else {
            return Err(BridgeError::NotAnObject);
        };
        let statement =
            CapabilityStatement::from_json(&object, &mut Path::root("CapabilityStatement"))
                .map_err(|source| BridgeError::Decode {
                    expected: "CapabilityStatement",
                    source: Box::new(source),
                })?;
        Ok((statement, raw))
    }

    /// Reads the bridge's build facts, pins and lanes from
    /// `GET {base}health/info`.
    ///
    /// # Errors
    /// Returns [`BridgeError::Refused`] for any status but `200`,
    /// [`BridgeError::Document`] for a body that is no
    /// `ferrobridge_operator_api` document of its kind, and the transport and
    /// size errors of [`Self::capability_statement`].
    pub async fn health_info(&self, token: &AccessToken) -> Result<Info, BridgeError> {
        self.document("health/info", "health info", token, &[StatusCode::OK])
            .await
            .map(|(_status, info)| info)
    }

    /// Reads the bridge's readiness from `GET {base}health/readiness`, with
    /// the status it answered: `200` while every indicator is up and `503`
    /// while one is down, each with the report naming every indicator.
    ///
    /// # Errors
    /// Returns [`BridgeError::Refused`] for any other status, and the body
    /// errors of [`Self::health_info`].
    pub async fn readiness(
        &self,
        token: &AccessToken,
    ) -> Result<(StatusCode, Readiness), BridgeError> {
        self.document(
            "health/readiness",
            "readiness report",
            token,
            &[StatusCode::OK, StatusCode::SERVICE_UNAVAILABLE],
        )
        .await
    }

    /// Reads the running load from the operator API, `GET
    /// {base}operator/state`.
    ///
    /// # Errors
    /// Returns [`BridgeError::Refused`] for any status but `200` (a `404`
    /// where the operator API is off, a `403` without the operator scope),
    /// and the body errors of [`Self::health_info`].
    pub async fn operator_state(&self, token: &AccessToken) -> Result<EngineState, BridgeError> {
        self.document("operator/state", "engine state", token, &[StatusCode::OK])
            .await
            .map(|(_status, state)| state)
    }

    /// Reads the configuration the bridge started on from the operator API,
    /// `GET {base}operator/configuration`, every secret by reference alone.
    ///
    /// # Errors
    /// Returns [`BridgeError::Refused`] for any status but `200` (a `404`
    /// where the operator API is off, a `403` without the operator scope),
    /// and the body errors of [`Self::health_info`].
    pub async fn configuration(&self, token: &AccessToken) -> Result<Configuration, BridgeError> {
        self.document(
            "operator/configuration",
            "running configuration",
            token,
            &[StatusCode::OK],
        )
        .await
        .map(|(_status, configuration)| configuration)
    }

    /// Reads the bridge's SMART discovery document, `GET
    /// {base}fhir/.well-known/smart-configuration` (SMART App Launch 2.2.0,
    /// conformance).
    ///
    /// # Errors
    /// Returns [`BridgeError::Refused`] for any status but `200` (a `404`
    /// where the bridge checks no tokens), and the transport and body errors
    /// of [`Self::capability_statement`].
    pub async fn smart_configuration(
        &self,
        token: &AccessToken,
    ) -> Result<serde_json::Value, BridgeError> {
        let (_status, value) = self
            .document::<serde_json::Value>(
                "fhir/.well-known/smart-configuration",
                "SMART discovery document",
                token,
                &[StatusCode::OK],
            )
            .await?;
        if value.is_object() {
            Ok(value)
        } else {
            Err(BridgeError::NotAnObject)
        }
    }

    /// Sends `GET {base}{route}` for JSON as the operator whose access token
    /// is `token`, and reads the document the bridge answers with a status in
    /// `admitted` into `T`; any other status is a refusal.
    async fn document<T: serde::de::DeserializeOwned>(
        &self,
        route: &'static str,
        expected: &'static str,
        token: &AccessToken,
        admitted: &[StatusCode],
    ) -> Result<(StatusCode, T), BridgeError> {
        let (status, _headers, body) = self.fetch(self.url(route)?, JSON, token, admitted).await?;
        let document = serde_json::from_slice(&body)
            .map_err(|source| BridgeError::Document { expected, source })?;
        Ok((status, document))
    }

    /// Lists every mapping file through the operator API, `GET
    /// {base}operator/mappings`, with each file's digest, owner and
    /// diagnostics.
    ///
    /// # Errors
    /// Returns [`BridgeError::Refused`] for any status but `200`, and the
    /// body errors of [`Self::health_info`].
    pub async fn mapping_files(&self, token: &AccessToken) -> Result<Vec<Listed>, BridgeError> {
        self.document(
            "operator/mappings",
            "mapping listing",
            token,
            &[StatusCode::OK],
        )
        .await
        .map(|(_status, listing)| listing)
    }

    /// Reads one mapping file's verbatim bytes through the operator API,
    /// `GET {base}operator/mappings/{path}`, with the digest its `ETag`
    /// names.
    ///
    /// `path` is a wire path of plain segments (`fhirconnect/models/a.yml`);
    /// each segment is percent-encoded onto the URL on its own.
    ///
    /// # Errors
    /// Returns [`BridgeError::Path`] for a path with an empty, `.` or `..`
    /// segment, before anything is sent; [`BridgeError::Refused`] for any
    /// status but `200`; and the transport and size errors of
    /// [`Self::capability_statement`].
    pub async fn mapping_file(
        &self,
        path: &str,
        token: &AccessToken,
    ) -> Result<MappingFile, BridgeError> {
        if path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return Err(BridgeError::Path);
        }
        let mut url = self.url("operator/mappings/")?;
        url.path_segments_mut()
            .map_err(|()| BridgeError::Path)?
            .pop_if_empty()
            .extend(path.split('/'));
        let (_status, headers, bytes) = self.fetch(url, YAML, token, &[StatusCode::OK]).await?;
        let etag = headers
            .get(http::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.trim_matches('"').to_owned());
        Ok(MappingFile { bytes, etag })
    }

    /// Sends `GET url` for `accept` as the operator whose access token is
    /// `token`, and reads the headers and body the bridge answers with a
    /// status in `admitted`; any other status is a refusal.
    async fn fetch(
        &self,
        url: Url,
        accept: &'static str,
        token: &AccessToken,
        admitted: &[StatusCode],
    ) -> Result<(StatusCode, http::HeaderMap, Vec<u8>), BridgeError> {
        self.exchange(self.client.get(url), accept, token, admitted)
            .await
    }

    /// Sends `POST url` with `body` in `content_type`, for `accept`, as
    /// [`Self::fetch`] does a `GET`.
    async fn post(
        &self,
        url: Url,
        (content_type, body): (&'static str, String),
        accept: &'static str,
        token: &AccessToken,
        admitted: &[StatusCode],
    ) -> Result<(StatusCode, http::HeaderMap, Vec<u8>), BridgeError> {
        let request = self
            .client
            .post(url)
            .header(http::header::CONTENT_TYPE, content_type)
            .body(body);
        self.exchange(request, accept, token, admitted).await
    }

    /// Sends `request` for `accept` with the operator's bearer token, and
    /// reads the headers and body of an answer whose status is in
    /// `admitted`; any other status is a refusal.
    async fn exchange(
        &self,
        request: reqwest::RequestBuilder,
        accept: &'static str,
        token: &AccessToken,
        admitted: &[StatusCode],
    ) -> Result<(StatusCode, http::HeaderMap, Vec<u8>), BridgeError> {
        let mut bearer = HeaderValue::from_str(&format!("Bearer {}", token.0.expose_secret()))
            .map_err(|_invalid| BridgeError::Token)?;
        bearer.set_sensitive(true);
        let response = request
            .header(ACCEPT, accept)
            .header(AUTHORIZATION, bearer)
            .send()
            .await
            .map_err(|source| BridgeError::Transport { source })?;
        let status = response.status();
        if !admitted.contains(&status) {
            let body = read(response, REFUSAL_LIMIT, Overflow::Truncate).await?;
            return Err(BridgeError::Refused {
                status,
                body: String::from_utf8_lossy(&body).into_owned(),
            });
        }
        let headers = response.headers().clone();
        let body = read(response, BODY_LIMIT, Overflow::Refuse).await?;
        Ok((status, headers, body))
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
