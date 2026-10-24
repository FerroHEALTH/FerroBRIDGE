// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator API: the engine state and the mapping files, read by an
//! operator console under the operator scope.
//!
//! The API is mounted under `/operator` when `[operator] enabled` is true,
//! which the configuration refuses without `[auth]`, and every route passes
//! the layer of [`guard::layer`]. `GET /operator/state` answers what the running load
//! compiled, `GET /operator/mappings` every mapping file with its digest and
//! owner, `GET /operator/mappings/{path}` one file's verbatim bytes with its
//! digest as the `ETag`, and `GET /operator/configuration` the configuration
//! the bridge started on, every secret by reference alone
//! ([`configuration`]). With `[operator] mapping_dir`, `POST
//! /operator/mappings/check` checks a draft over the running set, `PUT
//! /operator/mappings/{path}` saves one and reloads, and `POST
//! /operator/reload` reloads the files on disk ([`save`], [`reload`]). No
//! specification governs the operator surface: our own design.

pub mod configuration;
pub mod files;
pub mod guard;
pub mod reload;
pub mod save;
pub mod snapshot;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;

use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use http::StatusCode;
use http::header;

use crate::operator::files::Root;
use crate::operator::snapshot::Snapshot;

/// The media type a mapping file is served with.
const YAML: &str = "application/yaml";

/// The operator API, resolved from `[operator]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorSettings {
    /// The writable directory console saves go to, when one is configured.
    pub mapping_dir: Option<PathBuf>,
}

/// What the operator routes read and write.
#[derive(Debug)]
pub struct Operator {
    /// The provisioned mapping directories, then the console's.
    roots: Vec<Root>,
    /// The running load, swapped whole on a reload.
    snapshot: RwLock<Arc<Snapshot>>,
    /// What a reload compiles against, when saving is configured.
    reloader: Option<reload::Reloader>,
    /// The writable directory console saves go to.
    mapping_dir: Option<PathBuf>,
    /// Held for the whole of a check, a save or a reload, so they never
    /// interleave.
    saving: tokio::sync::Mutex<()>,
    /// The configuration the bridge started on, when it is answered.
    configuration: Option<ferrobridge_operator_api::configuration::Configuration>,
}

impl Operator {
    /// Returns the read-only operator surface over `roots` and the running
    /// load `snapshot` describes.
    #[must_use]
    pub fn new(roots: Vec<Root>, snapshot: Snapshot) -> Self {
        Self {
            roots,
            snapshot: RwLock::new(Arc::new(snapshot)),
            reloader: None,
            mapping_dir: None,
            saving: tokio::sync::Mutex::new(()),
            configuration: None,
        }
    }

    /// Returns this surface answering `configuration` as the running
    /// configuration.
    #[must_use]
    pub fn with_configuration(
        mut self,
        configuration: ferrobridge_operator_api::configuration::Configuration,
    ) -> Self {
        self.configuration = Some(configuration);
        self
    }

    /// Returns this surface saving into `mapping_dir` and reloading through
    /// `reloader`.
    ///
    /// The console's files live under `mapping_dir/fhirconnect` and
    /// `mapping_dir/omocl` and join the set beside the provisioned ones.
    #[must_use]
    pub fn with_saving(mut self, mapping_dir: PathBuf, reloader: reload::Reloader) -> Self {
        for language in [files::Language::Fhirconnect, files::Language::Omocl] {
            self.roots.push(Root::console(
                language,
                language.console_directory(&mapping_dir),
            ));
        }
        self.mapping_dir = Some(mapping_dir);
        self.reloader = Some(reloader);
        self
    }

    /// Returns the mapping directories, provisioned first.
    #[must_use]
    pub fn roots(&self) -> &[Root] {
        &self.roots
    }

    /// Returns the provisioned mapping directories alone.
    #[must_use]
    pub fn provisioned_roots(&self) -> Vec<Root> {
        self.roots
            .iter()
            .filter(|root| root.owner() == files::Owner::Provisioned)
            .cloned()
            .collect()
    }

    /// Returns the running load, taken once per request.
    #[must_use]
    pub fn snapshot(&self) -> Arc<Snapshot> {
        // NOTE: no specification governs this: our own design; the lock only
        // guards an `Arc` swap, so a poisoned lock still holds a whole set.
        Arc::clone(
            &self
                .snapshot
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Replaces the running load every later request reads.
    pub fn replace(&self, snapshot: Snapshot) {
        *self
            .snapshot
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::new(snapshot);
    }

    /// Returns `report` as a check answers it, each file by its wire path and
    /// `draft` by the path it stands for.
    #[must_use]
    pub fn load_report(
        &self,
        report: &crate::check::Report,
        draft: Option<&reload::Draft>,
    ) -> ferrobridge_operator_api::operator::LoadReport {
        ferrobridge_operator_api::operator::LoadReport {
            clean: report.is_clean(),
            counts: report.counts(),
            diagnostics: report
                .diagnostics()
                .iter()
                .map(|diagnostic| {
                    crate::check::shown_diagnostic(
                        diagnostic,
                        &reload::shown(&self.roots, draft, diagnostic.file()),
                    )
                })
                .collect(),
        }
    }
}

/// Returns the operator routes over `operator`, every one guarded by the
/// gate of `smart` and its operator scope.
pub fn routes(operator: Arc<Operator>, smart: &crate::smart::Smart) -> Router {
    Router::new()
        .route("/operator/state", get(state))
        .route("/operator/configuration", get(running_configuration))
        .route("/operator/mappings", get(mappings))
        .route("/operator/mappings/check", post(save::check))
        .route("/operator/mappings/{*path}", get(mapping).put(save::save))
        .route("/operator/reload", post(save::reload))
        .with_state(operator)
        .route_layer(guard::layer(smart))
}

/// `GET /operator/state`: the running load.
async fn state(State(operator): State<Arc<Operator>>) -> Response {
    axum::Json(operator.snapshot().document(&operator.roots)).into_response()
}

/// `GET /operator/configuration`: the configuration the bridge started on.
async fn running_configuration(State(operator): State<Arc<Operator>>) -> Response {
    match operator.configuration.as_ref() {
        Some(configuration) => axum::Json(configuration).into_response(),
        None => guard::refusal(
            StatusCode::NOT_FOUND,
            None,
            "not-found",
            "this operator API answers no running configuration",
        ),
    }
}

/// `GET /operator/mappings`: every mapping file with its digest and owner.
async fn mappings(State(operator): State<Arc<Operator>>) -> Response {
    blocking(move || match operator.snapshot().listing(&operator.roots) {
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
