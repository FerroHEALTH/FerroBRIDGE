// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping-set revisions the console reads and applies.
//!
//! The history, one revision with its diff against the running one, and the
//! apply of a revision under `If-Match` on the running one with its reason.
//! No specification governs the operator surface: our own design.

use ferrobridge_operator_api::revisions::{Revision, RevisionDiff, Revisions};
use http::StatusCode;

use crate::bridge::editing::{REASON, reason_header};
use crate::bridge::{AccessToken, Bridge, BridgeError, JSON};
use crate::model::Answered;

impl Bridge {
    /// Reads every revision of the mapping set, `GET
    /// {base}operator/revisions`, newest first.
    ///
    /// # Errors
    /// Returns [`BridgeError::Refused`] for any status but `200` (a `409`
    /// where the bridge saves nothing), and the body errors of
    /// [`Self::health_info`].
    pub async fn revisions(&self, token: &AccessToken) -> Result<Revisions, BridgeError> {
        self.document(
            "operator/revisions",
            "revision listing",
            token,
            &[StatusCode::OK],
        )
        .await
        .map(|(_status, revisions)| revisions)
    }

    /// Reads one revision, `GET {base}operator/revisions/{id}`.
    ///
    /// # Errors
    /// Returns [`BridgeError::Path`] when the base takes no path segments,
    /// [`BridgeError::Refused`] for any status but `200` (a `404` for an id
    /// no revision has), and the body errors of [`Self::health_info`].
    pub async fn revision(&self, id: &str, token: &AccessToken) -> Result<Revision, BridgeError> {
        let body = self.revision_document(id, None, token).await?;
        serde_json::from_slice(&body).map_err(|source| BridgeError::Document {
            expected: "revision",
            source,
        })
    }

    /// Reads how one revision differs from the running one, `GET
    /// {base}operator/revisions/{id}/diff`.
    ///
    /// # Errors
    /// As [`Self::revision`].
    pub async fn revision_diff(
        &self,
        id: &str,
        token: &AccessToken,
    ) -> Result<RevisionDiff, BridgeError> {
        let body = self.revision_document(id, Some("diff"), token).await?;
        serde_json::from_slice(&body).map_err(|source| BridgeError::Document {
            expected: "revision diff",
            source,
        })
    }

    /// Reads the revision `id`, or its `tail` document.
    async fn revision_document(
        &self,
        id: &str,
        tail: Option<&str>,
        token: &AccessToken,
    ) -> Result<Vec<u8>, BridgeError> {
        let mut url = self.url("operator/revisions")?;
        {
            let mut segments = url.path_segments_mut().map_err(|()| BridgeError::Path)?;
            segments.push(id);
            if let Some(tail) = tail {
                segments.push(tail);
            }
        }
        let (_status, _headers, body) = self.fetch(url, JSON, token, &[StatusCode::OK]).await?;
        Ok(body)
    }

    /// Applies the revision `id` over the running revision `running` with
    /// `reason`, `POST {base}operator/revisions/{id}/apply`; the bridge
    /// compiles the set with its files first and changes nothing when it
    /// does not compile.
    ///
    /// A `200` and a `422` are answers with the set's report; a `412` (the
    /// running revision changed), `428`, `404` or `409` is a refusal.
    ///
    /// # Errors
    /// Returns [`BridgeError::Reason`] for a reason that is empty, over
    /// [`crate::bridge::editing::MAX_REASON`] characters or not printable ASCII on one line,
    /// [`BridgeError::Refused`] for any other status, and the transport and
    /// body errors of [`Self::capability_statement`].
    pub async fn apply_revision(
        &self,
        id: &str,
        running: &str,
        reason: &str,
        token: &AccessToken,
    ) -> Result<Answered, BridgeError> {
        let reason = reason_header(reason)?;
        let mut url = self.url("operator/revisions")?;
        url.path_segments_mut()
            .map_err(|()| BridgeError::Path)?
            .push(id)
            .push("apply");
        let request = self
            .client
            .post(url)
            .header(REASON, reason)
            .header(http::header::IF_MATCH, format!("\"{running}\""));
        self.write(request, token).await
    }
}

/// The media type of a mapping-set archive.
pub const ZIP: &str = "application/zip";

impl Bridge {
    /// Reads the revision `id` as a ZIP, `GET
    /// {base}operator/revisions/{id}/export`.
    ///
    /// # Errors
    /// Returns [`BridgeError::Path`] when the base takes no path segments,
    /// [`BridgeError::Refused`] for any status but `200`, and the transport
    /// errors of [`Self::capability_statement`].
    pub async fn export_revision(
        &self,
        id: &str,
        token: &AccessToken,
    ) -> Result<Vec<u8>, BridgeError> {
        let mut url = self.url("operator/revisions")?;
        url.path_segments_mut()
            .map_err(|()| BridgeError::Path)?
            .push(id)
            .push("export");
        let (_status, _headers, body) = self.fetch(url, ZIP, token, &[StatusCode::OK]).await?;
        Ok(body)
    }

    /// Imports the ZIP `archive` with `reason` as an unapplied revision,
    /// `POST {base}operator/revisions/import`; the bridge checks its files
    /// first and records nothing when the set does not compile with them.
    ///
    /// A `201` (the `ETag` names the new revision) and a `422` are answers
    /// with the check's report; a `400` (no archive the bridge reads) or a
    /// `409` is a refusal.
    ///
    /// # Errors
    /// As [`Self::apply_revision`].
    pub async fn import_revision(
        &self,
        archive: Vec<u8>,
        reason: &str,
        token: &AccessToken,
    ) -> Result<Answered, BridgeError> {
        let reason = reason_header(reason)?;
        let url = self.url("operator/revisions/import")?;
        let request = self
            .client
            .post(url)
            .header(http::header::CONTENT_TYPE, ZIP)
            .header(REASON, reason)
            .body(archive);
        self.write(request, token).await
    }
}
