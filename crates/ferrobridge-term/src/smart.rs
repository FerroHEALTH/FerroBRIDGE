// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The token of `ferrohealth-smart`'s OAuth 2.0 client credentials grant
//! (RFC 6749 §4.4) as the client's [`TokenProvider`] (feature `smart`).
//!
//! `ferrohealth-smart`'s `TokenSource` fetches the token, caches it and
//! renews it before it expires. [`SmartToken`] hands the client the bare
//! token of its cached `Authorization` value and, when the server refuses
//! one with `401`, drops it from the cache, so the one repeated request
//! carries a token fetched anew (RFC 6750 §3.1). Every product that
//! authorizes its terminology calls with that grant takes it in place of an
//! adapter of its own.

use std::sync::Arc;

use ferrohealth_smart::client::TokenError;
use ferrohealth_smart::client::cache::TokenSource;
use http::HeaderValue;
use secrecy::ExposeSecret;
use secrecy::SecretString;

use crate::config::TokenFailure;
use crate::config::TokenProvider;

/// The scheme of an RFC 6750 §2.1 bearer credential, with its separator.
const BEARER: &str = "Bearer ";

/// A client credentials token source as the client's [`TokenProvider`].
#[derive(Debug, Clone)]
pub struct SmartToken(TokenSource);

impl From<TokenSource> for SmartToken {
    fn from(source: TokenSource) -> Self {
        Self(source)
    }
}

#[async_trait::async_trait]
impl TokenProvider for SmartToken {
    async fn token(&self) -> Result<SecretString, TokenFailure> {
        let token = self
            .0
            .token()
            .await
            .map_err(|error| TokenFailure::new(error))?;
        token
            .header()
            .to_str()
            .ok()
            .and_then(|value| value.strip_prefix(BEARER))
            .map(SecretString::from)
            .ok_or_else(|| TokenFailure::new(Arc::new(TokenError::Unusable)))
    }

    fn invalidate(&self, stale: &SecretString) {
        // NOTE: RFC 6750 §2.1, the cached token travels as `Bearer <token>`, so
        // the refused one is dropped by that value.
        if let Ok(header) = HeaderValue::from_str(&format!("{BEARER}{}", stale.expose_secret())) {
            self.0.invalidate(&header);
        }
    }
}
