// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The authorization server metadata the resource server reads at boot.

use http::StatusCode;
use serde::Deserialize;
use url::Url;

use crate::auth::AuthError;
use crate::cdr::oauth::endpoint::metadata_locations;

/// The members of an authorization server metadata document the resource
/// server reads or republishes (RFC 8414 §2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct ServerMetadata {
    /// The issuer identifier the document describes.
    pub(crate) issuer: String,
    /// The key set URL.
    pub(crate) jwks_uri: Option<String>,
    /// The authorization endpoint.
    pub(crate) authorization_endpoint: Option<String>,
    /// The token endpoint.
    pub(crate) token_endpoint: Option<String>,
    /// The client authentication methods of the token endpoint.
    pub(crate) token_endpoint_auth_methods_supported: Option<Vec<String>>,
    /// The grant types the server supports.
    pub(crate) grant_types_supported: Option<Vec<String>>,
    /// The dynamic client registration endpoint.
    pub(crate) registration_endpoint: Option<String>,
    /// The scopes the server supports.
    pub(crate) scopes_supported: Option<Vec<String>>,
    /// The response types the server supports.
    pub(crate) response_types_supported: Option<Vec<String>>,
    /// The token introspection endpoint (RFC 7662).
    pub(crate) introspection_endpoint: Option<String>,
    /// The token revocation endpoint (RFC 7009).
    pub(crate) revocation_endpoint: Option<String>,
    /// The PKCE challenge methods the server supports (RFC 7636).
    pub(crate) code_challenge_methods_supported: Option<Vec<String>>,
}

/// Returns the metadata of `issuer`.
///
/// The RFC 8414 location is read first; a `404` there moves on to the
/// `OpenID` Connect Discovery 1.0 §4 location, and any other answer is final.
/// The document must name `issuer` itself (RFC 8414 §3.3).
pub(super) async fn discover(
    http: &reqwest::Client,
    issuer: &str,
) -> Result<ServerMetadata, AuthError> {
    let parsed: Url = issuer
        .parse()
        .map_err(|source| AuthError::IssuerUrl { source })?;
    let [oauth, openid] = metadata_locations(&parsed);
    let (url, status, body) = match get(http, oauth).await? {
        (url, status, _) if status == StatusCode::NOT_FOUND => {
            tracing::debug!(%url, "no RFC 8414 metadata; reading the OpenID Connect location");
            get(http, openid).await?
        }
        answered => answered,
    };
    if status != StatusCode::OK {
        return Err(AuthError::Discovery { url, status, body });
    }
    let metadata: ServerMetadata =
        serde_json::from_str(&body).map_err(|source| AuthError::Metadata { url, source })?;
    if metadata.issuer != issuer {
        return Err(AuthError::IssuerMismatch {
            expected: issuer.to_owned(),
            found: metadata.issuer,
        });
    }
    Ok(metadata)
}

/// Returns the status and body `url` answers a `GET` with.
pub(super) async fn get(
    http: &reqwest::Client,
    url: Url,
) -> Result<(Url, StatusCode, String), AuthError> {
    let response = http
        .get(url.clone())
        .header(http::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|source| AuthError::Transport {
            url: url.clone(),
            source,
        })?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|source| AuthError::Transport {
            url: url.clone(),
            source,
        })?;
    Ok((url, status, body))
}
