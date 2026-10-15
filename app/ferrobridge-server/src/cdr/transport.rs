// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The engine one call sends through: the configured `reqwest` engine, plus
//! the headers the generated parameters do not carry, a record of the last
//! answer, and one renewal of an OAuth token the CDR refused.
//!
//! No specification governs this: our own design. The generated parameters
//! carry no `X-Request-Id` and a read's parameters no `Prefer`, so the engine
//! adds both; the generated outcomes carry only the response headers the
//! `OpenAPI` documents declare, so the engine keeps the whole answer for the
//! ones they leave out (the `ETag` of an `adl2` template, the `ETag` of a
//! committed contribution whose body does not decode). The generated client
//! retries no `401`, so a refused token is renewed and the request re-sent
//! here, once.

use std::sync::Mutex;
use std::sync::PoisonError;

use http::HeaderMap;
use http::StatusCode;
use http::header::AUTHORIZATION;
use openehr_its::rest::client::ReqwestTransport;
use openehr_its::rest::client::Transport;
use openehr_its::rest::client::TransportError;

use crate::cdr::error::Upstream;
use crate::cdr::oauth::RenewalError;
use crate::cdr::oauth::cache::TokenSource;

/// The engine of one call.
#[derive(Debug)]
pub(crate) struct Scoped<'t> {
    /// The configured engine, with its pool and its timeout.
    inner: &'t ReqwestTransport,
    /// The headers every attempt carries beside the generated ones.
    headers: HeaderMap,
    /// The token cache a refused token is renewed from, under OAuth.
    renewal: Option<TokenSource>,
    /// The last answer an attempt read.
    answered: Mutex<Option<Upstream>>,
}

impl<'t> Scoped<'t> {
    /// Returns an engine over `inner` that adds `headers` to every attempt
    /// and renews a refused token from `renewal`.
    pub(crate) fn new(
        inner: &'t ReqwestTransport,
        headers: HeaderMap,
        renewal: Option<TokenSource>,
    ) -> Self {
        Self {
            inner,
            headers,
            renewal,
            answered: Mutex::new(None),
        }
    }

    /// Returns the last answer an attempt read, when one was read.
    pub(crate) fn answered(&self) -> Option<Upstream> {
        // NOTE: no specification governs this: our own design; a poisoned slot
        // still holds the last answer read, so it is taken as it stands.
        self.answered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Returns the answer to `again` under a renewed token, the token the
    /// refused attempt carried dropped first.
    async fn renewed(
        &self,
        source: &TokenSource,
        mut again: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Vec<u8>>, TransportError> {
        if let Some(stale) = again.headers().get(AUTHORIZATION) {
            source.invalidate(stale);
        }
        let token = source
            .token()
            .await
            .map_err(|source| TransportError::Send {
                source: Box::new(RenewalError::new(source)),
            })?;
        tracing::debug!("the CDR refused the token; re-sending once under a renewed one");
        again
            .headers_mut()
            .insert(AUTHORIZATION, token.header().clone());
        self.inner.send(again).await
    }
}

#[async_trait::async_trait]
impl Transport for Scoped<'_> {
    async fn send(
        &self,
        mut request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Vec<u8>>, TransportError> {
        for (name, value) in &self.headers {
            request.headers_mut().append(name.clone(), value.clone());
        }
        let again = self.renewal.as_ref().map(|_| copy(&request));
        let mut response = self.inner.send(request).await?;
        // NOTE: RFC 9110 §15.5.2, a `401` means the request was not applied,
        // so re-sending it once, a `POST` included, cannot apply it twice.
        if response.status() == StatusCode::UNAUTHORIZED
            && let (Some(source), Some(again)) = (self.renewal.as_ref(), again)
        {
            response = self.renewed(source, again).await?;
        }
        let upstream = Upstream::new(
            response.status(),
            response.headers().clone(),
            String::from_utf8_lossy(response.body()).into_owned(),
        );
        *self.answered.lock().unwrap_or_else(PoisonError::into_inner) = Some(upstream);
        Ok(response)
    }
}

/// Returns a copy of `request` that can be sent again.
fn copy(request: &http::Request<Vec<u8>>) -> http::Request<Vec<u8>> {
    let mut copy = http::Request::new(request.body().clone());
    *copy.method_mut() = request.method().clone();
    *copy.uri_mut() = request.uri().clone();
    *copy.version_mut() = request.version();
    *copy.headers_mut() = request.headers().clone();
    *copy.extensions_mut() = request.extensions().clone();
    copy
}
