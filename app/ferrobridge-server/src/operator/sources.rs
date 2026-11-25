// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `/operator/sources`: the inbound sources, created, changed, disabled and
//! deleted without a restart.
//!
//! `GET` lists every source, and one source with its `ETag` and its
//! versions; `POST` creates one; `PUT` and `DELETE` take `If-Match` with the
//! `ETag` they replace, and a stale one answers `412` (RFC 9110 §13.1.1); a
//! missing one answers `428` (RFC 6585 §3). Disabling is a `PUT` with
//! `"enabled": false`. The committer of every version is the subject of the
//! operator's token. No specification governs the operator surface: our own
//! design.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::get;
use ferrobridge_operator_api::sources::{Sources as Listing, Versions, Write};
use uuid::Uuid;

use super::guard;
use super::save::subject_of;
use crate::facade::reply::Refusal;
use crate::problem::{Kind, refuse, refuse_with};
use crate::sources::Sources;
use crate::sources::document::Document;
use crate::sources::store::WriteError;

/// The sources a request reads and writes, `None` without `[state]`.
type Held = State<Option<Arc<Sources>>>;

/// The routes of `/operator/sources`, behind the operator guard of `smart`.
pub fn routes(sources: Option<Arc<Sources>>, smart: &crate::smart::Smart) -> Router {
    Router::new()
        .route("/operator/sources", get(list).post(create))
        .route(
            "/operator/sources/{id}",
            get(one).put(change).delete(remove),
        )
        .route("/operator/sources/{id}/versions", get(versions))
        .with_state(sources)
        .route_layer(guard::layer(smart))
}

/// The `404` of a bridge without a state store.
const NO_STORE: &str = "this bridge keeps no inbound sources: [state] is not configured";

/// The `404` of an id no source has.
const NO_SOURCE: &str = "no source has this id";

/// Returns the `ETag` of `version`.
fn etag(version: i64) -> HeaderValue {
    HeaderValue::from_str(&format!("\"{version}\"")).unwrap_or(HeaderValue::from_static("\"0\""))
}

/// What `If-Match` asks a write to replace: any version for `*`, one
/// version, or nothing that names a version this bridge writes.
enum Matched {
    /// `If-Match: *`.
    Any,
    /// One strong `ETag`.
    Version(i64),
    /// No `If-Match`.
    Missing,
    /// An `ETag` this bridge never wrote, which no version matches.
    Foreign,
}

/// Returns what the `If-Match` of `headers` asks for.
fn matched(headers: &HeaderMap) -> Matched {
    let Some(value) = headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
    else {
        return Matched::Missing;
    };
    if value == "*" {
        return Matched::Any;
    }
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .and_then(|version| version.parse::<i64>().ok())
        .map_or(Matched::Foreign, Matched::Version)
}

/// Returns the version the `If-Match` of `headers` names, `None` for `*`,
/// or the refusal of a write whose `If-Match` is `missing` or foreign.
fn expected(headers: &HeaderMap, missing: &'static str) -> Result<Option<i64>, Refusal> {
    match matched(headers) {
        Matched::Missing => Err(Refusal::new(refuse(Kind::PreconditionRequired, missing))),
        Matched::Foreign => Err(Refusal::new(refused(&WriteError::Stale { current: 0 }))),
        Matched::Any => Ok(None),
        Matched::Version(version) => Ok(Some(version)),
    }
}

/// Returns the document `write` carries, checked against its kind, or the
/// refusal of an invalid one.
fn validated(sources: &Sources, write: &Write) -> Result<Document, Refusal> {
    sources
        .validate(write.kind, &write.document)
        .map_err(|invalid| Refusal::new(refused(&WriteError::Invalid(invalid))))
}

/// Returns the response of a refused or failed write.
fn refused(error: &WriteError) -> Response {
    match error {
        WriteError::Invalid(invalid) => refuse_with(Kind::InvalidSource, invalid.0.clone()),
        WriteError::Exists => refuse(Kind::SourceConflict, "another source is named so"),
        WriteError::RouteTaken => refuse(
            Kind::SourceConflict,
            "another source routes one of these MLLP senders",
        ),
        WriteError::NotFound => refuse(Kind::NotFound, NO_SOURCE),
        WriteError::Stale { .. } => refuse(
            Kind::PreconditionFailed,
            "the source changed since the version If-Match names",
        ),
        WriteError::KindChanged => refuse(Kind::InvalidSource, "a change keeps the source's kind"),
        WriteError::Store(cause) => unavailable(cause),
    }
}

/// Returns the `503` of a read that failed.
fn unavailable(error: &sqlx::Error) -> Response {
    tracing::error!(reason = %crate::indicators::reason(error), "the source store failed");
    refuse(
        Kind::SourcesUnavailable,
        "the inbound sources could not be read or written",
    )
}

/// Returns the source `id` names, `None` for text that is no id.
fn id_of(id: &str) -> Option<Uuid> {
    Uuid::parse_str(id).ok()
}

/// Returns `source` with its `ETag`, answered with `status`.
fn answered(source: &crate::sources::Source, status: StatusCode) -> Response {
    let mut response = (status, axum::Json(source.document())).into_response();
    response
        .headers_mut()
        .insert(header::ETAG, etag(source.version));
    response
}

/// Logs one write of a source on the operator audit target.
fn audited(subject: &str, action: &str, id: Uuid, status: StatusCode) {
    tracing::info!(
        target: guard::AUDIT_TARGET,
        subject,
        action,
        source = %id,
        status = status.as_u16(),
        "operator"
    );
}

/// `GET /operator/sources`: every source at its head version, by name.
async fn list(State(sources): Held) -> Response {
    let Some(sources) = sources else {
        return refuse(Kind::NotFound, NO_STORE);
    };
    match sources.store().list().await {
        Ok(listed) => axum::Json(Listing {
            sources: listed
                .iter()
                .map(crate::sources::Source::document)
                .collect(),
        })
        .into_response(),
        Err(error) => unavailable(&error),
    }
}

/// `GET /operator/sources/{id}`: one source with its `ETag`.
async fn one(State(sources): Held, Path(id): Path<String>) -> Response {
    let (Some(sources), Some(id)) = (sources, id_of(&id)) else {
        return refuse(Kind::NotFound, NO_SOURCE);
    };
    match sources.store().get(id).await {
        Ok(Some(source)) => answered(&source, StatusCode::OK),
        Ok(None) => refuse(Kind::NotFound, NO_SOURCE),
        Err(error) => unavailable(&error),
    }
}

/// `GET /operator/sources/{id}/versions`: every version of one source, a
/// deleted one included, oldest first.
async fn versions(State(sources): Held, Path(id): Path<String>) -> Response {
    let (Some(sources), Some(id)) = (sources, id_of(&id)) else {
        return refuse(Kind::NotFound, NO_SOURCE);
    };
    match sources.store().versions(id).await {
        Ok(versions) if versions.is_empty() => refuse(Kind::NotFound, NO_SOURCE),
        Ok(versions) => axum::Json(Versions { versions }).into_response(),
        Err(error) => unavailable(&error),
    }
}

/// `POST /operator/sources`: creates a source as version 1; `201` with its
/// `Location` and `ETag`.
async fn create(
    State(sources): Held,
    access: ferrohealth_smart::resource::Access,
    axum::Json(write): axum::Json<Write>,
) -> Result<Response, Refusal> {
    let sources = sources.ok_or_else(|| Refusal::new(refuse(Kind::NotFound, NO_STORE)))?;
    let document = validated(&sources, &write)?;
    let subject = subject_of(&access);
    let created = sources
        .store()
        .create(&write, &document, subject)
        .await
        .map_err(|error| Refusal::new(refused(&error)))?;
    audited(subject, "create-source", created.id, StatusCode::CREATED);
    let mut response = answered(&created, StatusCode::CREATED);
    if let Ok(location) = HeaderValue::from_str(&format!("/operator/sources/{}", created.id)) {
        response.headers_mut().insert(header::LOCATION, location);
    }
    Ok(response)
}

/// `PUT /operator/sources/{id}`: changes a source to its next version under
/// `If-Match`.
async fn change(
    State(sources): Held,
    Path(id): Path<String>,
    headers: HeaderMap,
    access: ferrohealth_smart::resource::Access,
    axum::Json(write): axum::Json<Write>,
) -> Result<Response, Refusal> {
    let (sources, id) = held(sources, &id)?;
    let expected = expected(
        &headers,
        "a change names the version it replaces in If-Match",
    )?;
    let document = validated(&sources, &write)?;
    let subject = subject_of(&access);
    let changed = sources
        .store()
        .update(id, expected, &write, &document, subject)
        .await
        .map_err(|error| Refusal::new(refused(&error)))?;
    audited(subject, "change-source", id, StatusCode::OK);
    Ok(answered(&changed, StatusCode::OK))
}

/// `DELETE /operator/sources/{id}`: deletes a source under `If-Match`,
/// keeping its versions; `204`.
async fn remove(
    State(sources): Held,
    Path(id): Path<String>,
    headers: HeaderMap,
    access: ferrohealth_smart::resource::Access,
) -> Result<Response, Refusal> {
    let (sources, id) = held(sources, &id)?;
    let expected = expected(
        &headers,
        "a deletion names the version it replaces in If-Match",
    )?;
    let subject = subject_of(&access);
    sources
        .store()
        .delete(id, expected, subject)
        .await
        .map_err(|error| Refusal::new(refused(&error)))?;
    audited(subject, "delete-source", id, StatusCode::NO_CONTENT);
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Returns the sources and the source id `id` names, or the `404` of a
/// bridge that keeps no sources or an id that names none.
fn held(sources: Option<Arc<Sources>>, id: &str) -> Result<(Arc<Sources>, Uuid), Refusal> {
    match (sources, id_of(id)) {
        (Some(sources), Some(id)) => Ok((sources, id)),
        _ => Err(Refusal::new(refuse(Kind::NotFound, NO_SOURCE))),
    }
}
