// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every facade interaction reads of the request it serves.
//!
//! A route extracts one [`Entry`] and reads its [`Context`], and a batch
//! builds one context per entry, so an interaction takes it whole and never a
//! list of its parts (no specification governs this: our own design).

use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::response::IntoResponse;
use axum::response::Response;
use ferrohealth_smart::resource::Access;
use http::HeaderMap;
use http::Uri;
use http::request::Parts;

use crate::facade::Facade;

/// The facade serving a request, the caller the layer admitted, and the
/// request's headers and URL.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// The facade serving the request.
    pub facade: &'a Facade,
    /// What the layer of [`crate::facade::guard`] admitted the caller with.
    pub access: &'a Access,
    /// The request's headers.
    pub headers: &'a HeaderMap,
    /// The request's URL.
    pub uri: &'a Uri,
}

impl<'a> Context<'a> {
    /// Returns the context of one request to `facade` by `access`.
    #[must_use]
    pub const fn new(
        facade: &'a Facade,
        access: &'a Access,
        (headers, uri): (&'a HeaderMap, &'a Uri),
    ) -> Self {
        Self {
            facade,
            access,
            headers,
            uri,
        }
    }
}

/// The parts of one request a facade route reads, extracted once at its
/// entry: the facade, the caller the layer admitted, the headers and the URL.
#[derive(Debug)]
pub struct Entry {
    /// The facade serving the request.
    facade: Arc<Facade>,
    /// What the layer admitted the caller with.
    access: Access,
    /// The request's headers.
    headers: HeaderMap,
    /// The request's URL.
    uri: Uri,
}

impl Entry {
    /// Returns the request's [`Context`].
    #[must_use]
    pub fn context(&self) -> Context<'_> {
        Context::new(&self.facade, &self.access, (&self.headers, &self.uri))
    }
}

impl FromRequestParts<Arc<Facade>> for Entry {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        facade: &Arc<Facade>,
    ) -> Result<Self, Self::Rejection> {
        let access = Access::from_request_parts(parts, facade)
            .await
            .map_err(IntoResponse::into_response)?;
        Ok(Self {
            facade: Arc::clone(facade),
            access,
            headers: parts.headers.clone(),
            uri: parts.uri.clone(),
        })
    }
}
