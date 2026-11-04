// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The export and import of a mapping set: `GET
//! /operator/revisions/{id}/export` and `POST /operator/revisions/import`.
//!
//! An export answers a revision's console files as one ZIP
//! ([`crate::operator::revisions::archive`]). An import takes a ZIP posted
//! raw as `application/zip` with its reason, checks its files with the
//! provisioned ones as a dry run, and records them as a revision that does
//! not run until an apply: a set that does not compile is refused with its
//! diagnostics and nothing is recorded. No specification governs the
//! operator surface: our own design.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::Path;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use ferrobridge_operator_api::operator::LoadReport;
use ferrobridge_operator_api::revisions::{Origin, Revision};
use ferrohealth_smart::resource::Access;
use http::HeaderMap;
use http::StatusCode;
use http::header;

use crate::mappings::files;
use crate::operator::Operator;
use crate::operator::guard::AUDIT_TARGET;
use crate::operator::reload::ReloadError;
use crate::operator::revisions::Entry;
use crate::operator::revisions::apply::{Checked, Staged};
use crate::operator::revisions::archive;
use crate::operator::save::{Failure, OnBlockingThread, blocking, reason, subject_of, unavailable};
use crate::problem::{Kind, refuse};

/// The media type of a mapping-set archive.
pub const ZIP: &str = "application/zip";

/// `GET /operator/revisions/{id}/export`: the revision's console files as
/// one ZIP, the same bytes at every export of the same files.
pub async fn export(State(operator): State<Arc<Operator>>, Path(id): Path<String>) -> Response {
    let exported = blocking(Arc::clone(&operator), move |operator, _thread| {
        let store = operator.revisions().ok_or(Failure::Unconfigured)?;
        let revision = store
            .get(&id)
            .map_err(Failure::Store)?
            .ok_or(Failure::UnknownRevision)?;
        let contents = store.contents(&revision).map_err(Failure::Store)?;
        let bytes = archive::export(&contents).map_err(Failure::Archive)?;
        Ok((revision.id, bytes))
    })
    .await;
    match exported {
        Ok((id, bytes)) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, String::from(ZIP)),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"ferrobridge-mappings-{id}.zip\""),
                ),
                (header::ETAG, format!("\"{id}\"")),
            ],
            bytes,
        )
            .into_response(),
        Err(failure) => failure.into_response(),
    }
}

/// How an import ended, short of a failure.
#[derive(Debug)]
pub enum Imported {
    /// The set compiled with the archive's files, which are recorded as an
    /// unapplied revision.
    Recorded {
        /// The revision recorded.
        revision: Revision,
        /// The report of the check.
        report: LoadReport,
    },
    /// The set does not compile with the archive's files; nothing is
    /// recorded.
    Rejected {
        /// The report naming every diagnostic.
        report: LoadReport,
    },
    /// The check could not run to the end.
    Unavailable {
        /// Why it stopped.
        error: ReloadError,
    },
}

/// `POST /operator/revisions/import`: checks the ZIP in the body as a dry
/// run and records it as an unapplied revision.
///
/// It answers `201` with the check's report, the new revision's address and
/// its id as the `ETag`, and `422` with the diagnostics when the set does not
/// compile with the archive's files.
pub async fn import(
    State(operator): State<Arc<Operator>>,
    access: Access,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let subject = subject_of(&access).to_owned();
    let Some(reason) = reason(&headers) else {
        return refuse(
            Kind::ReasonRequired,
            "an import names its reason in the Ferrobridge-Reason header, one line of at most 200 characters",
        );
    };
    let zipped = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(ZIP));
    if !zipped {
        return Failure::Archive(archive::ArchiveError::BadEntry).into_response();
    }
    let digest = files::sha256(&body);
    let imported = blocking(Arc::clone(&operator), {
        let (subject, reason) = (subject.clone(), reason.clone());
        move |operator, thread| operator.import_blocking(thread, &body, &subject, &reason)
    })
    .await;
    let (recorded, files, response) = match imported {
        Ok(Imported::Recorded { revision, report }) => {
            let response = (
                StatusCode::CREATED,
                [
                    (
                        header::LOCATION,
                        format!("/operator/revisions/{}", revision.id),
                    ),
                    (header::ETAG, format!("\"{}\"", revision.id)),
                ],
                axum::Json(report),
            )
                .into_response();
            (Some(revision.id), revision.files.len(), response)
        }
        Ok(Imported::Rejected { report }) => (
            None,
            0,
            (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(report)).into_response(),
        ),
        Ok(Imported::Unavailable { error }) => (None, 0, unavailable(&error)),
        Err(failure) => (None, 0, failure.into_response()),
    };
    tracing::info!(
        target: AUDIT_TARGET,
        subject = subject.as_str(),
        action = "import",
        archive_sha256 = digest.as_str(),
        files = files,
        recorded = recorded.as_deref().unwrap_or("-"),
        reason = reason.as_str(),
        status = response.status().as_u16(),
        "operator"
    );
    response
}

impl Operator {
    /// Checks the files of `archive` with the provisioned ones and records
    /// them as an unapplied revision under `author` and `reason`.
    pub(crate) fn import_blocking(
        &self,
        thread: &OnBlockingThread,
        archive: &[u8],
        author: &str,
        reason: &str,
    ) -> Result<Imported, Failure> {
        let (Some(mapping_dir), Some(store)) = (self.mapping_dir.as_deref(), self.revisions())
        else {
            return Err(Failure::Unconfigured);
        };
        let contents = archive::import(archive).map_err(Failure::Archive)?;
        let staged = Staged::write(mapping_dir, &contents)?;
        let report = match self.check_staged(thread, &staged) {
            Ok(prepared) => {
                crate::operator::load_report_over(prepared.report(), &staged.roots(self), None)
            }
            Err(Checked::Rejected(report)) => return Ok(Imported::Rejected { report }),
            Err(Checked::Unavailable(error)) => return Ok(Imported::Unavailable { error }),
        };
        drop(staged);
        let revision = store
            .record(
                Entry {
                    origin: Origin::Import,
                    author: author.to_owned(),
                    reason: reason.to_owned(),
                    applied: false,
                    source: None,
                },
                &contents,
            )
            .map_err(Failure::Store)?;
        Ok(Imported::Recorded { revision, report })
    }
}
