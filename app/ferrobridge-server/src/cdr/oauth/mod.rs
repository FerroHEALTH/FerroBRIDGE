// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The CDR credential the bridge obtains for itself with the OAuth 2.0 client
//! credentials grant (RFC 6749 §4.4), cached and renewed before it expires.
//!
//! The bridge reaches the CDR under its own token for the CDR's audience and
//! never forwards a caller's token, because an access token names the resource
//! server it is for (RFC 9068 §4). The token endpoint is configured outright
//! or discovered from the issuer's metadata (RFC 8414 §3, with the `OpenID`
//! Connect Discovery 1.0 §4 location as the fallback). [`cache::TokenSource`]
//! fetches on first use, shares one fetch among concurrent callers, and hands
//! the generated ITS-REST client a bearer credential per attempt.

pub mod cache;
pub(crate) mod endpoint;

use std::sync::Arc;
use std::time::Duration;

use http::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;
use url::Url;

/// Where the token endpoint comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// The issuer identifier, whose metadata names the token endpoint
    /// (RFC 8414 §2). Kept as configured, because the metadata's `issuer`
    /// must be identical to it (RFC 8414 §3.3).
    Issuer(String),
    /// The token endpoint itself (RFC 6749 §3.2).
    Token(Url),
}

/// How the client authenticates to the token endpoint (RFC 6749 §2.3.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuth {
    /// HTTP Basic over the form-encoded client id and secret, the scheme every
    /// authorization server must support (RFC 6749 §2.3.1).
    #[default]
    ClientSecretBasic,
    /// The client id and secret as `client_id` and `client_secret` in the
    /// request body (RFC 6749 §2.3.1).
    ClientSecretPost,
}

/// Everything the bridge needs to obtain its CDR token.
///
/// The secret is a [`SecretString`], so a `Debug` rendering never carries it.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    /// Where the token endpoint comes from.
    pub endpoint: Endpoint,
    /// The client identifier (RFC 6749 §2.2).
    pub client_id: String,
    /// The client secret.
    pub client_secret: SecretString,
    /// How the client authenticates to the token endpoint.
    pub client_auth: ClientAuth,
    /// The `scope` request parameter (RFC 6749 §3.3), when the deployment
    /// asks for one.
    pub scope: Option<String>,
    /// The `audience` request parameter some authorization servers read in
    /// place of a resource indicator; no specification defines it for this
    /// grant, so it is sent only when configured.
    pub audience: Option<String>,
    /// The `resource` request parameter (RFC 8707 §2).
    pub resource: Option<Url>,
    /// How long before the token expires a new one is fetched.
    pub refresh_margin: Duration,
}

impl OAuthConfig {
    /// Returns a configuration for `client_id` with `client_secret` at
    /// `endpoint`.
    ///
    /// The client authenticates with HTTP Basic, sends no `scope`, `audience`
    /// or `resource`, and renews its token 30 seconds before it expires.
    #[must_use]
    pub fn new(endpoint: Endpoint, client_id: String, client_secret: SecretString) -> Self {
        Self {
            endpoint,
            client_id,
            client_secret,
            client_auth: ClientAuth::default(),
            scope: None,
            audience: None,
            resource: None,
            refresh_margin: Duration::from_secs(30),
        }
    }
}

/// The error response of a token endpoint (RFC 6749 §5.2).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct OAuthError {
    /// The error code: `invalid_request`, `invalid_client`, `invalid_grant`,
    /// `unauthorized_client`, `unsupported_grant_type` or `invalid_scope`.
    pub error: String,
    /// The human-readable description, when the server sent one.
    pub error_description: Option<String>,
    /// The page describing the error, when the server sent one.
    pub error_uri: Option<String>,
}

impl std::fmt::Display for OAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.error_description.as_deref() {
            Some(description) => write!(f, "{} ({description})", self.error),
            None => f.write_str(&self.error),
        }
    }
}

/// A token the bridge could not obtain.
///
/// No variant carries the client secret or a token.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TokenError {
    /// The HTTP engine of the token client could not be built.
    #[error("the HTTP engine of the token client could not be built")]
    Build {
        /// What `reqwest` reported.
        #[source]
        source: reqwest::Error,
    },
    /// A request to the authorization server never completed, its timeout
    /// included.
    #[error("the request to {url} could not be completed")]
    Transport {
        /// The URL that was called.
        url: Url,
        /// What `reqwest` reported.
        #[source]
        source: reqwest::Error,
    },
    /// No metadata document answered at either discovery location.
    #[error("the authorization server metadata at {url} answered {status}")]
    Discovery {
        /// The last location that was tried.
        url: Url,
        /// The status it answered.
        status: StatusCode,
        /// The body it answered, as received.
        body: String,
    },
    /// The metadata document is not the JSON object RFC 8414 §3.2 describes.
    #[error("the authorization server metadata at {url} is not a metadata document")]
    Metadata {
        /// The location that answered it.
        url: Url,
        /// What the JSON reader reported.
        #[source]
        source: serde_json::Error,
    },
    /// The metadata names an issuer other than the configured one, which
    /// RFC 8414 §3.3 requires the client to refuse.
    #[error("the authorization server metadata names issuer {found:?}, not {expected:?}")]
    IssuerMismatch {
        /// The configured issuer.
        expected: String,
        /// The issuer the metadata named.
        found: String,
    },
    /// The configured issuer is not a URL.
    #[error("the configured issuer is not a URL")]
    IssuerUrl {
        /// What the URL parser reported.
        #[source]
        source: url::ParseError,
    },
    /// The metadata names a token endpoint that is not a URL.
    #[error("the authorization server metadata names a token endpoint that is not a URL")]
    TokenEndpointUrl {
        /// What the URL parser reported.
        #[source]
        source: url::ParseError,
    },
    /// The token endpoint answered anything other than `200`, with the OAuth
    /// error when the body carried one (RFC 6749 §5.2).
    #[error("the token endpoint answered {status}{}", oauth_clause(error.as_ref()))]
    Refused {
        /// The status it answered.
        status: StatusCode,
        /// The OAuth error, when the body was one.
        error: Option<OAuthError>,
        /// The body, as received.
        body: String,
    },
    /// The `200` body is not the access token response of RFC 6749 §5.1.
    #[error("the token endpoint answered a body that is not an access token response")]
    Malformed {
        /// What the JSON reader reported.
        #[source]
        source: serde_json::Error,
    },
    /// The token is not a bearer token, the one type the bridge sends
    /// (RFC 6749 §7.1, RFC 6750).
    #[error("the token endpoint issued a token of type {found:?}; the bridge sends bearer tokens")]
    TokenType {
        /// The `token_type` the response named.
        found: String,
    },
    /// The token cannot travel as an RFC 6750 bearer credential.
    #[error("the issued token cannot travel as a bearer credential")]
    Unusable {
        /// The rule the token breaks.
        #[source]
        source: openehr_its::rest::client::InvalidCredentials,
    },
    /// The fetch ended without an outcome.
    #[error("the token fetch ended without an outcome")]
    Abandoned,
}

impl TokenError {
    /// Returns the status the authorization server answered, when the failure
    /// carries one.
    #[must_use]
    pub const fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Discovery { status, .. } | Self::Refused { status, .. } => Some(*status),
            _ => None,
        }
    }
}

/// The CDR refused a token (`401`) and no new one could be obtained.
///
/// It reaches a caller as the source of the generated client's transport
/// refusal, because the renewal runs inside the call's engine.
#[derive(Debug, thiserror::Error)]
#[error("the CDR refused the token (401) and no new token could be obtained")]
pub struct RenewalError {
    /// Why the new token could not be obtained.
    #[source]
    source: Arc<TokenError>,
}

impl RenewalError {
    /// Returns the renewal failure `source` caused.
    #[must_use]
    pub const fn new(source: Arc<TokenError>) -> Self {
        Self { source }
    }

    /// Returns why the new token could not be obtained.
    #[must_use]
    pub fn token_error(&self) -> &TokenError {
        &self.source
    }
}

/// Returns the OAuth clause of a refusal's display.
fn oauth_clause(error: Option<&OAuthError>) -> String {
    error.map_or_else(String::new, |error| format!(": {error}"))
}

#[cfg(test)]
mod tests {
    use super::{OAuthError, TokenError};
    use http::StatusCode;

    #[test]
    fn a_refusal_displays_the_oauth_error_and_its_description() {
        let error = TokenError::Refused {
            status: StatusCode::UNAUTHORIZED,
            error: Some(OAuthError {
                error: String::from("invalid_client"),
                error_description: Some(String::from("unknown client")),
                error_uri: None,
            }),
            body: String::new(),
        };
        assert_eq!(
            "the token endpoint answered 401 Unauthorized: invalid_client (unknown client)",
            error.to_string()
        );
        assert_eq!(Some(StatusCode::UNAUTHORIZED), error.status());
    }
}
