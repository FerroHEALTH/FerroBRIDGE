// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The token the bridge obtains for itself with the OAuth 2.0 client
//! credentials grant (RFC 6749 §4.4), handed to the CDR client and the
//! terminology client.
//!
//! `ferrohealth-smart` fetches, caches and renews the token
//! ([`TokenSource`]); this module hands it to the two clients in the form
//! each one asks for: the generated ITS-REST client's
//! [`CredentialsProvider`] and the terminology client's [`TokenProvider`].
//! A token an upstream refuses with `401` is dropped and asked for once more
//! by the client that met the refusal.

use std::sync::Arc;

use ferrobridge_term::config::TokenFailure;
use ferrobridge_term::config::TokenProvider;
use ferrohealth_smart::client::ClientCredentials;
use ferrohealth_smart::client::TokenError;
use ferrohealth_smart::client::cache::Token;
use ferrohealth_smart::client::cache::TokenSource;
use http::HeaderValue;
use openehr_its::rest::client::Credentials;
use openehr_its::rest::client::CredentialsError;
use openehr_its::rest::client::CredentialsProvider;
use secrecy::ExposeSecret;
use secrecy::SecretString;

/// The scheme of an RFC 6750 §2.1 bearer credential, with its separator.
const BEARER: &str = "Bearer ";

/// The upstream token of one lane, cheap to clone: clones share one cache.
#[derive(Debug, Clone)]
pub struct UpstreamToken {
    /// The cache the client credentials grant fills.
    source: TokenSource,
}

impl UpstreamToken {
    /// Returns the token of the grant `credentials` describes.
    ///
    /// Nothing is fetched until a client asks for the token.
    ///
    /// # Errors
    ///
    /// Returns [`TokenError`] when the HTTP engine of the token client cannot
    /// be built.
    pub fn new(credentials: ClientCredentials) -> Result<Self, TokenError> {
        TokenSource::new(credentials).map(Self::from)
    }

    /// Returns the cache, for the renewal of a token the CDR refused.
    #[must_use]
    pub const fn source(&self) -> &TokenSource {
        &self.source
    }
}

impl From<TokenSource> for UpstreamToken {
    fn from(source: TokenSource) -> Self {
        Self { source }
    }
}

/// Returns the token `token` renders, the `Authorization` value without its
/// scheme.
fn bare(token: &Token) -> Result<SecretString, Arc<TokenError>> {
    token
        .header()
        .to_str()
        .ok()
        .and_then(|value| value.strip_prefix(BEARER))
        .map(SecretString::from)
        .ok_or_else(|| Arc::new(TokenError::Unusable))
}

#[async_trait::async_trait]
impl CredentialsProvider for UpstreamToken {
    async fn credentials(&self) -> Result<Credentials, CredentialsError> {
        let token = self.source.token().await.map_err(CredentialsError::new)?;
        bare(&token)
            .map(Credentials::bearer)
            .map_err(CredentialsError::new)
    }
}

#[async_trait::async_trait]
impl TokenProvider for UpstreamToken {
    async fn token(&self) -> Result<SecretString, TokenFailure> {
        let token = self.source.token().await.map_err(failure)?;
        bare(&token).map_err(failure)
    }

    fn invalidate(&self, stale: &SecretString) {
        // NOTE: RFC 6750 §2.1, the token the cache holds travels as
        // `Bearer <token>`, so the refused one is dropped by that value.
        if let Ok(header) = HeaderValue::from_str(&format!("{BEARER}{}", stale.expose_secret())) {
            self.source.invalidate(&header);
        }
    }
}

/// Returns the terminology client's failure carrying `error`, which a caller
/// walking the chain reaches as the [`TokenError`] it is.
fn failure(error: Arc<TokenError>) -> TokenFailure {
    TokenFailure::new(error)
}
