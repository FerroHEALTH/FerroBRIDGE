// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator API's writes the editor calls: a draft checked over the
//! running set, a save under `If-Match` with its reason, and a reload.
//!
//! A draft's bytes travel in the request body as the editor holds them, and
//! no line here logs them. No specification governs the operator surface:
//! our own design.

use ferrobridge_operator_api::operator::LoadReport;
use http::{HeaderValue, StatusCode};

use crate::bridge::{AccessToken, Bridge, BridgeError, JSON, YAML};
use crate::model::Answered;

/// The header a save names its reason in.
pub const REASON: &str = "ferrobridge-reason";

/// The longest reason a save may give, in characters.
pub const MAX_REASON: usize = 200;

/// What a save asks of the file it replaces (RFC 9110 §13.1.1, §13.1.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Precondition {
    /// The file exists with this digest, the one the editor opened.
    Match(String),
    /// The file does not exist yet.
    Absent,
}

impl Bridge {
    /// Checks `draft` standing in for the file at `path` over the running
    /// set, `POST {base}operator/mappings/check?path=…`; nothing is written.
    ///
    /// # Errors
    /// Returns [`BridgeError::Path`] for a path that is not plain segments,
    /// [`BridgeError::Refused`] for any status but `200`, and the transport
    /// and body errors of [`Self::capability_statement`].
    pub async fn check_mapping(
        &self,
        path: &str,
        draft: Vec<u8>,
        token: &AccessToken,
    ) -> Result<Answered, BridgeError> {
        plain(path)?;
        let mut url = self.url("operator/mappings/check")?;
        url.query_pairs_mut().append_pair("path", path);
        let request = self
            .client
            .post(url)
            .header(http::header::CONTENT_TYPE, YAML)
            .body(draft);
        let (status, _headers, body) = self
            .exchange(request, JSON, token, &[StatusCode::OK])
            .await?;
        Ok(Answered {
            status,
            report: report(&body)?,
            etag: None,
        })
    }

    /// Saves `draft` as the console file at `path` under `precondition` with
    /// `reason`, `PUT {base}operator/mappings/{path}`; the bridge compiles
    /// the set with it first and writes nothing when it does not compile.
    ///
    /// A `200` and a `422` are answers with the set's report; a `412` (the
    /// file changed), `428`, `409` (a provisioned file holds the path) or
    /// `400` is a refusal.
    ///
    /// # Errors
    /// Returns [`BridgeError::Path`] for a path that is not plain segments,
    /// [`BridgeError::Reason`] for a reason that is empty, over
    /// [`MAX_REASON`] characters or not printable ASCII on one line,
    /// [`BridgeError::Refused`] for any other status, and the transport and
    /// body errors of [`Self::capability_statement`].
    pub async fn save_mapping(
        &self,
        path: &str,
        draft: Vec<u8>,
        precondition: &Precondition,
        reason: &str,
        token: &AccessToken,
    ) -> Result<Answered, BridgeError> {
        plain(path)?;
        let reason = reason.trim();
        if reason.is_empty()
            || reason.chars().count() > MAX_REASON
            || !reason.chars().all(|c| c.is_ascii_graphic() || c == ' ')
        {
            return Err(BridgeError::Reason);
        }
        let reason = HeaderValue::from_str(reason).map_err(|_invalid| BridgeError::Reason)?;
        let mut url = self.url("operator/mappings/")?;
        url.path_segments_mut()
            .map_err(|()| BridgeError::Path)?
            .pop_if_empty()
            .extend(path.split('/'));
        let mut request = self
            .client
            .put(url)
            .header(http::header::CONTENT_TYPE, YAML)
            .header(REASON, reason);
        request = match precondition {
            Precondition::Match(digest) => {
                request.header(http::header::IF_MATCH, format!("\"{digest}\""))
            }
            Precondition::Absent => request.header(http::header::IF_NONE_MATCH, "*"),
        };
        let (status, headers, body) = self
            .exchange(
                request.body(draft),
                JSON,
                token,
                &[StatusCode::OK, StatusCode::UNPROCESSABLE_ENTITY],
            )
            .await?;
        let etag = headers
            .get(http::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.trim_matches('"').to_owned());
        Ok(Answered {
            status,
            report: report(&body)?,
            etag,
        })
    }

    /// Reloads the mapping files on disk, `POST {base}operator/reload`.
    ///
    /// # Errors
    /// Returns [`BridgeError::Refused`] for any status but `200` and `422`,
    /// and the transport and body errors of [`Self::capability_statement`].
    pub async fn reload_mappings(&self, token: &AccessToken) -> Result<Answered, BridgeError> {
        let url = self.url("operator/reload")?;
        let (status, _headers, body) = self
            .exchange(
                self.client.post(url),
                JSON,
                token,
                &[StatusCode::OK, StatusCode::UNPROCESSABLE_ENTITY],
            )
            .await?;
        Ok(Answered {
            status,
            report: report(&body)?,
            etag: None,
        })
    }
}

/// Refuses a wire path with an empty, `.` or `..` segment.
fn plain(path: &str) -> Result<(), BridgeError> {
    if path
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        Err(BridgeError::Path)
    } else {
        Ok(())
    }
}

/// The report `body` holds, read into its `ferrobridge_operator_api`
/// document.
fn report(body: &[u8]) -> Result<LoadReport, BridgeError> {
    serde_json::from_slice(body).map_err(|source| BridgeError::Document {
        expected: "load report",
        source,
    })
}
