// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbound sources the console lists, shows and edits.
//!
//! Every source, one source with its `ETag` and its versions, and its
//! create, change and delete, a change and a delete under `If-Match` on the
//! `ETag` the page showed. No specification governs the operator surface:
//! our own design.

use ferrobridge_operator_api::sources::{Source, Sources, Versions, Write};
use http::StatusCode;
use url::Url;

use crate::bridge::{AccessToken, Bridge, BridgeError, JSON};

/// A source with the `ETag` a change or a delete names.
#[derive(Debug, Clone, PartialEq)]
pub struct Tagged {
    /// The source.
    pub source: Source,
    /// Its `ETag`, quotes included.
    pub etag: String,
}

impl Bridge {
    /// Reads every source, `GET {base}operator/sources`.
    ///
    /// # Errors
    /// Returns [`BridgeError::Refused`] for any status but `200` (a `404`
    /// where the bridge keeps no sources), and the body errors of
    /// [`Self::health_info`].
    pub async fn sources(&self, token: &AccessToken) -> Result<Sources, BridgeError> {
        self.document(
            "operator/sources",
            "source listing",
            token,
            &[StatusCode::OK],
        )
        .await
        .map(|(_status, sources)| sources)
    }

    /// The URL of the source `id`, with `tail` after it when given.
    fn source_url(&self, id: &str, tail: Option<&str>) -> Result<Url, BridgeError> {
        let mut url = self.url("operator/sources")?;
        {
            let mut segments = url.path_segments_mut().map_err(|()| BridgeError::Path)?;
            segments.push(id);
            if let Some(tail) = tail {
                segments.push(tail);
            }
        }
        Ok(url)
    }

    /// Reads one source with its `ETag`, `GET {base}operator/sources/{id}`.
    ///
    /// # Errors
    /// Returns [`BridgeError::Path`] when the base takes no path segments,
    /// [`BridgeError::Refused`] for any status but `200` (a `404` for an id
    /// no source has), and the body errors of [`Self::health_info`].
    pub async fn source(&self, id: &str, token: &AccessToken) -> Result<Tagged, BridgeError> {
        let (_status, headers, body) = self
            .exchange(
                self.client.get(self.source_url(id, None)?),
                JSON,
                token,
                &[StatusCode::OK],
            )
            .await?;
        tagged(&headers, &body)
    }

    /// Reads every version of one source, `GET
    /// {base}operator/sources/{id}/versions`.
    ///
    /// # Errors
    /// As [`Self::source`].
    pub async fn source_versions(
        &self,
        id: &str,
        token: &AccessToken,
    ) -> Result<Versions, BridgeError> {
        let url = self.source_url(id, Some("versions"))?;
        let (_status, _headers, body) = self
            .exchange(self.client.get(url), JSON, token, &[StatusCode::OK])
            .await?;
        serde_json::from_slice(&body).map_err(|source| BridgeError::Document {
            expected: "source versions",
            source,
        })
    }

    /// Creates the source `write` describes, `POST {base}operator/sources`.
    ///
    /// # Errors
    /// Returns [`BridgeError::Refused`] for any status but `201` (a `422`
    /// for a document the bridge refuses, a `409` for a name another source
    /// holds), and the body errors of [`Self::health_info`].
    pub async fn create_source(
        &self,
        write: &Write,
        token: &AccessToken,
    ) -> Result<Tagged, BridgeError> {
        let request = self.client.post(self.url("operator/sources")?).json(write);
        let (_status, headers, body) = self
            .exchange(request, JSON, token, &[StatusCode::CREATED])
            .await?;
        tagged(&headers, &body)
    }

    /// Changes the source `id` at `etag` to what `write` describes, `PUT
    /// {base}operator/sources/{id}` with `If-Match`.
    ///
    /// # Errors
    /// As [`Self::create_source`], with `200` the answer admitted and a
    /// `412` for a source another writer changed since `etag`.
    pub async fn change_source(
        &self,
        id: &str,
        etag: &str,
        write: &Write,
        token: &AccessToken,
    ) -> Result<Tagged, BridgeError> {
        let request = self
            .client
            .put(self.source_url(id, None)?)
            .header(http::header::IF_MATCH, etag)
            .json(write);
        let (_status, headers, body) = self
            .exchange(request, JSON, token, &[StatusCode::OK])
            .await?;
        tagged(&headers, &body)
    }

    /// Deletes the source `id` at `etag`, `DELETE
    /// {base}operator/sources/{id}` with `If-Match`.
    ///
    /// # Errors
    /// Returns [`BridgeError::Refused`] for any status but `204` (a `412`
    /// for a source another writer changed since `etag`), and the transport
    /// errors of [`Self::health_info`].
    pub async fn delete_source(
        &self,
        id: &str,
        etag: &str,
        token: &AccessToken,
    ) -> Result<(), BridgeError> {
        let request = self
            .client
            .delete(self.source_url(id, None)?)
            .header(http::header::IF_MATCH, etag);
        self.exchange(request, JSON, token, &[StatusCode::NO_CONTENT])
            .await
            .map(|_answer| ())
    }
}

/// Returns the source `body` holds with the `ETag` of `headers`.
fn tagged(headers: &http::HeaderMap, body: &[u8]) -> Result<Tagged, BridgeError> {
    let source = serde_json::from_slice(body).map_err(|source| BridgeError::Document {
        expected: "source",
        source,
    })?;
    let etag = headers
        .get(http::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    Ok(Tagged { source, etag })
}
