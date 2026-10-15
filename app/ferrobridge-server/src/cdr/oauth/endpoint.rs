// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two calls to the authorization server: the metadata read that finds
//! the token endpoint, and the client credentials token request.

use http::StatusCode;
use secrecy::ExposeSecret;
use serde::Deserialize;
use url::Url;

use crate::cdr::oauth::ClientAuth;
use crate::cdr::oauth::OAuthConfig;
use crate::cdr::oauth::OAuthError;
use crate::cdr::oauth::TokenError;

/// The RFC 8414 §3 well-known suffix.
const OAUTH_METADATA: &str = "/.well-known/oauth-authorization-server";

/// The `OpenID` Connect Discovery 1.0 §4 well-known suffix.
const OPENID_CONFIGURATION: &str = "/.well-known/openid-configuration";

/// The members of an authorization server metadata document the bridge reads
/// (RFC 8414 §2).
#[derive(Debug, Deserialize)]
struct Metadata {
    /// The issuer identifier the document describes.
    issuer: String,
    /// The token endpoint URL.
    token_endpoint: String,
}

/// The members of an access token response the bridge reads (RFC 6749 §5.1).
#[derive(Deserialize)]
pub(super) struct TokenResponse {
    /// The access token.
    pub(super) access_token: String,
    /// The token type, compared case-insensitively.
    pub(super) token_type: String,
    /// The lifetime of the token in seconds, when the server sent one.
    pub(super) expires_in: Option<u64>,
}

impl std::fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenResponse")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .finish_non_exhaustive()
    }
}

/// Returns the two metadata locations of `issuer`, the RFC 8414 one first.
///
/// RFC 8414 §3.1 inserts the suffix between the host and the path, after
/// removing a terminating `/`; `OpenID` Connect Discovery 1.0 §4 appends it to
/// the issuer, after removing a terminating `/`.
pub(super) fn metadata_locations(issuer: &Url) -> [Url; 2] {
    let path = issuer.path().trim_end_matches('/');
    let mut oauth = issuer.clone();
    oauth.set_path(&format!("{OAUTH_METADATA}{path}"));
    let mut openid = issuer.clone();
    openid.set_path(&format!("{path}{OPENID_CONFIGURATION}"));
    [oauth, openid]
}

/// Returns the token endpoint the metadata of `issuer` names.
///
/// The RFC 8414 location is read first; a `404` there moves on to the
/// `OpenID` Connect location, and any other answer is final.
pub(super) async fn discover(http: &reqwest::Client, issuer: &str) -> Result<Url, TokenError> {
    let parsed: Url = issuer
        .parse()
        .map_err(|source| TokenError::IssuerUrl { source })?;
    let [oauth, openid] = metadata_locations(&parsed);
    let (url, status, body) = match get(http, oauth).await? {
        (url, status, _) if status == StatusCode::NOT_FOUND => {
            tracing::debug!(%url, "no RFC 8414 metadata; reading the OpenID Connect location");
            get(http, openid).await?
        }
        answered => answered,
    };
    if status != StatusCode::OK {
        return Err(TokenError::Discovery { url, status, body });
    }
    let metadata: Metadata =
        serde_json::from_str(&body).map_err(|source| TokenError::Metadata { url, source })?;
    if metadata.issuer != issuer {
        return Err(TokenError::IssuerMismatch {
            expected: issuer.to_owned(),
            found: metadata.issuer,
        });
    }
    metadata
        .token_endpoint
        .parse()
        .map_err(|source| TokenError::TokenEndpointUrl { source })
}

/// Returns the status and body `url` answers a `GET` with.
async fn get(http: &reqwest::Client, url: Url) -> Result<(Url, StatusCode, String), TokenError> {
    let response = http
        .get(url.clone())
        .header(http::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|source| TokenError::Transport {
            url: url.clone(),
            source,
        })?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|source| TokenError::Transport {
            url: url.clone(),
            source,
        })?;
    Ok((url, status, body))
}

/// Returns the access token response `endpoint` answers the client
/// credentials grant of `config` with (RFC 6749 §4.4.2 and §4.4.3).
pub(super) async fn request_token(
    http: &reqwest::Client,
    endpoint: &Url,
    config: &OAuthConfig,
) -> Result<TokenResponse, TokenError> {
    let mut request = http
        .post(endpoint.clone())
        .header(http::header::ACCEPT, "application/json");
    if config.client_auth == ClientAuth::ClientSecretBasic {
        // NOTE: RFC 6749 §2.3.1 form-encodes the id and the secret
        // (Appendix B) before they become the RFC 7617 user-pass.
        let user: String =
            url::form_urlencoded::byte_serialize(config.client_id.as_bytes()).collect();
        let password: String =
            url::form_urlencoded::byte_serialize(config.client_secret.expose_secret().as_bytes())
                .collect();
        request = request.basic_auth(user, Some(password));
    }
    let response = request
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(token_request_body(config))
        .send()
        .await
        .map_err(|source| TokenError::Transport {
            url: endpoint.clone(),
            source,
        })?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|source| TokenError::Transport {
            url: endpoint.clone(),
            source,
        })?;
    if status != StatusCode::OK {
        // NOTE: RFC 6749 §5.2 makes the error body JSON with `error`; a body
        // that is not one is legitimately absent, and the raw body stays.
        let error = serde_json::from_str::<OAuthError>(&body).ok();
        return Err(TokenError::Refused {
            status,
            error,
            body,
        });
    }
    serde_json::from_str(&body).map_err(|source| TokenError::Malformed { source })
}

/// Returns the form-encoded body of the token request of `config`
/// (RFC 6749 §4.4.2), the client's id and secret included under
/// `client_secret_post`.
fn token_request_body(config: &OAuthConfig) -> String {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    form.append_pair("grant_type", "client_credentials");
    if let Some(scope) = config.scope.as_deref() {
        form.append_pair("scope", scope);
    }
    if let Some(audience) = config.audience.as_deref() {
        form.append_pair("audience", audience);
    }
    if let Some(resource) = config.resource.as_ref() {
        form.append_pair("resource", resource.as_str());
    }
    if config.client_auth == ClientAuth::ClientSecretPost {
        form.append_pair("client_id", &config.client_id);
        form.append_pair("client_secret", config.client_secret.expose_secret());
    }
    form.finish()
}

#[cfg(test)]
mod tests {
    use super::metadata_locations;
    use url::Url;

    #[test]
    fn rfc_8414_inserts_the_suffix_and_openid_connect_appends_it() {
        let issuer: Url = "https://as.example.org/realms/ferro/"
            .parse()
            .expect("a valid URL");
        let [oauth, openid] = metadata_locations(&issuer);
        assert_eq!(
            "https://as.example.org/.well-known/oauth-authorization-server/realms/ferro",
            oauth.as_str()
        );
        assert_eq!(
            "https://as.example.org/realms/ferro/.well-known/openid-configuration",
            openid.as_str()
        );
    }

    #[test]
    fn an_issuer_without_a_path_reads_the_bare_suffixes() {
        let issuer: Url = "https://as.example.org".parse().expect("a valid URL");
        let [oauth, openid] = metadata_locations(&issuer);
        assert_eq!(
            "https://as.example.org/.well-known/oauth-authorization-server",
            oauth.as_str()
        );
        assert_eq!(
            "https://as.example.org/.well-known/openid-configuration",
            openid.as_str()
        );
    }
}
