// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator API: the engine state and the mapping files, read by an
//! operator console under the operator scope.
//!
//! The API is mounted under `/operator` when `[operator] enabled` is true,
//! which the configuration refuses without `[auth]`, and every route passes
//! [`guard::guard`]. `GET /operator/state` answers what the running load
//! compiled, `GET /operator/mappings` every mapping file with its digest and
//! owner, and `GET /operator/mappings/{path}` one file's verbatim bytes with
//! its digest as the `ETag`. No specification governs the operator surface:
//! our own design.

pub mod files;
pub mod guard;
pub mod snapshot;

use std::sync::Arc;

use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use http::StatusCode;
use http::header;

use crate::auth::Authority;
use crate::operator::files::Root;
use crate::operator::snapshot::Snapshot;

/// The media type a mapping file is served with.
const YAML: &str = "application/yaml";

/// What the operator routes read.
#[derive(Debug)]
pub struct Operator {
    /// The configured mapping directories.
    roots: Vec<Root>,
    /// The running load, as the boot check saw it.
    snapshot: Snapshot,
}

impl Operator {
    /// Returns the operator surface over `roots` and the running load
    /// `snapshot` describes.
    #[must_use]
    pub const fn new(roots: Vec<Root>, snapshot: Snapshot) -> Self {
        Self { roots, snapshot }
    }

    /// Returns the configured mapping directories.
    #[must_use]
    pub fn roots(&self) -> &[Root] {
        &self.roots
    }

    /// Returns the running load.
    #[must_use]
    pub const fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
}

/// Returns the operator routes over `operator`, every one guarded by
/// `authority`.
pub fn routes(operator: Arc<Operator>, authority: Arc<Authority>) -> Router {
    Router::new()
        .route("/operator/state", get(state))
        .route("/operator/mappings", get(mappings))
        .route("/operator/mappings/{*path}", get(mapping))
        .with_state(operator)
        .route_layer(axum::middleware::from_fn_with_state(
            authority,
            guard::guard,
        ))
}

/// `GET /operator/state`: the running load.
async fn state(State(operator): State<Arc<Operator>>) -> Response {
    axum::Json(operator.snapshot.document(&operator.roots)).into_response()
}

/// `GET /operator/mappings`: every mapping file with its digest and owner.
async fn mappings(State(operator): State<Arc<Operator>>) -> Response {
    blocking(move || match operator.snapshot.listing(&operator.roots) {
        Ok(listing) => axum::Json(listing).into_response(),
        Err(error) => unreadable(&error),
    })
    .await
}

/// `GET /operator/mappings/{path}`: one file's verbatim bytes.
async fn mapping(State(operator): State<Arc<Operator>>, Path(path): Path<String>) -> Response {
    blocking(move || read_mapping(&operator, &path)).await
}

/// Resolves and reads the file `path` names, on a blocking thread.
fn read_mapping(operator: &Operator, path: &str) -> Response {
    let located = match files::resolve(&operator.roots, path) {
        Ok(located) => located,
        Err(files::PathError::Read { path, source }) => {
            return unreadable(&crate::walk::Error { path, source });
        }
        Err(files::PathError::UnknownRoot | files::PathError::Step | files::PathError::Absent) => {
            return absent();
        }
    };
    match std::fs::read(located.path()) {
        Ok(bytes) => {
            let etag = format!("\"{}\"", files::sha256(&bytes));
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, YAML.to_owned()),
                    (header::ETAG, etag),
                ],
                bytes,
            )
                .into_response()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => absent(),
        Err(error) => unreadable(&crate::walk::Error {
            path: located.path().to_path_buf(),
            source: error,
        }),
    }
}

/// Runs `work`, which reads the file system, off the async runtime.
///
/// A task that panics answers `500` through the same refusal as a read fault.
async fn blocking(work: impl FnOnce() -> Response + Send + 'static) -> Response {
    match tokio::task::spawn_blocking(work).await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "an operator file read did not finish");
            guard::refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                None,
                "unreadable",
                "a mapping directory could not be read",
            )
        }
    }
}

/// Returns the `404` of a path that names no mapping file.
fn absent() -> Response {
    guard::refusal(
        StatusCode::NOT_FOUND,
        None,
        "not-found",
        "no mapping file is at this path",
    )
}

/// Returns the `500` of a mapping directory that could not be read.
fn unreadable(error: &crate::walk::Error) -> Response {
    tracing::error!(
        path = %error.path.display(),
        error = %error.source,
        "a mapping directory could not be read"
    );
    guard::refusal(
        StatusCode::INTERNAL_SERVER_ERROR,
        None,
        "unreadable",
        "a mapping directory could not be read",
    )
}
