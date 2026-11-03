// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The revision routes: `GET /operator/revisions`, `GET
//! /operator/revisions/{id}`, `GET /operator/revisions/{id}/diff` and `POST
//! /operator/revisions/{id}/apply`.
//!
//! An apply names the running revision in `If-Match` and its reason in
//! `Ferrobridge-Reason`, and its audit line names the caller, the revision
//! applied, the revision it replaced, the one it recorded and the reason. No
//! specification governs the operator surface: our own design.

use std::sync::Arc;

use axum::extract::Path;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use ferrobridge_operator_api::revisions::Revisions;
use ferrohealth_smart::resource::Access;
use http::HeaderMap;
use http::StatusCode;
use http::header;

use crate::operator::Operator;
use crate::operator::guard::AUDIT_TARGET;
use crate::operator::revisions::apply::Applied;
use crate::operator::revisions::diff::diff;
use crate::operator::save::{Failure, Precondition, blocking, reason, subject_of, unavailable};
use crate::problem::{Kind, refuse};

/// `GET /operator/revisions`: every revision, newest first, with the id of
/// the running one.
pub async fn list(State(operator): State<Arc<Operator>>) -> Response {
    let listed = blocking(Arc::clone(&operator), |operator| {
        let store = operator.revisions().ok_or(Failure::Unconfigured)?;
        let revisions = store.list().map_err(Failure::Store)?;
        let running = revisions
            .iter()
            .rev()
            .find(|revision| revision.applied)
            .map(|revision| revision.id.clone());
        Ok(Revisions {
            running,
            revisions: revisions
                .iter()
                .rev()
                .map(ferrobridge_operator_api::revisions::Revision::summary)
                .collect(),
        })
    })
    .await;
    match listed {
        Ok(revisions) => axum::Json(revisions).into_response(),
        Err(failure) => failure.into_response(),
    }
}

/// `GET /operator/revisions/{id}`: one revision whole.
pub async fn one(State(operator): State<Arc<Operator>>, Path(id): Path<String>) -> Response {
    let found = blocking(Arc::clone(&operator), move |operator| {
        let store = operator.revisions().ok_or(Failure::Unconfigured)?;
        store
            .get(&id)
            .map_err(Failure::Store)?
            .ok_or(Failure::UnknownRevision)
    })
    .await;
    match found {
        Ok(revision) => axum::Json(revision).into_response(),
        Err(failure) => failure.into_response(),
    }
}

/// `GET /operator/revisions/{id}/diff`: how the revision differs from the
/// running one, file by file.
pub async fn differences(
    State(operator): State<Arc<Operator>>,
    Path(id): Path<String>,
) -> Response {
    let found = blocking(Arc::clone(&operator), move |operator| {
        let store = operator.revisions().ok_or(Failure::Unconfigured)?;
        let to = store
            .get(&id)
            .map_err(Failure::Store)?
            .ok_or(Failure::UnknownRevision)?;
        let running = store.running().map_err(Failure::Store)?;
        diff(store, running.as_ref(), &to).map_err(Failure::Store)
    })
    .await;
    match found {
        Ok(diff) => axum::Json(diff).into_response(),
        Err(failure) => failure.into_response(),
    }
}

/// `POST /operator/revisions/{id}/apply`: applies the revision.
///
/// It replaces the console files by the revision's when `If-Match` names the running revision and the set
/// compiles with them, and reloads; `200` with the new set's report and the
/// recorded revision's id as the `ETag`, `422` with the diagnostics when it
/// does not compile.
pub async fn apply(
    State(operator): State<Arc<Operator>>,
    access: Access,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let subject = subject_of(&access).to_owned();
    let Some(reason) = reason(&headers) else {
        return refuse(
            Kind::ReasonRequired,
            "an apply names its reason in the Ferrobridge-Reason header, one line of at most 200 characters",
        );
    };
    let precondition = Precondition::of(&headers);
    let applied = blocking(Arc::clone(&operator), {
        let (id, subject, reason) = (id.clone(), subject.clone(), reason.clone());
        move |operator| operator.apply_blocking(&id, &precondition, &subject, &reason)
    })
    .await;
    let (running, recorded, response) = match applied {
        Ok((running, Applied::Done { revision, report })) => {
            let response = (
                StatusCode::OK,
                [(header::ETAG, format!("\"{}\"", revision.id))],
                axum::Json(report),
            )
                .into_response();
            (running, Some(revision.id), response)
        }
        Ok((running, Applied::Rejected { report })) => (
            running,
            None,
            (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(report)).into_response(),
        ),
        Ok((running, Applied::Unreloaded { error })) => (running, None, unavailable(&error)),
        Err(failure) => (None, None, failure.into_response()),
    };
    tracing::info!(
        target: AUDIT_TARGET,
        subject = subject.as_str(),
        action = "apply",
        revision = id.as_str(),
        running = running.as_deref().unwrap_or("-"),
        recorded = recorded.as_deref().unwrap_or("-"),
        reason = reason.as_str(),
        status = response.status().as_u16(),
        "operator"
    );
    response
}
