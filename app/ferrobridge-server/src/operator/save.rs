// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Checking a draft, saving it and reloading the set.
//!
//! A draft is written next to where it will live, compiled with every other
//! file of the set, and only a draft that compiles for every lane is renamed
//! over its target, after its bytes and the directory entry are synced, so a
//! reader sees the old file or the new one. A save goes to `[operator]
//! mapping_dir` only; a path a provisioned file holds is refused. The audit
//! line names the caller, the file, the two digests and the reason, never the
//! bytes. No specification governs the operator surface: our own design.

use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Extension;
use axum::body::Bytes;
use axum::extract::Query;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use http::HeaderMap;
use http::StatusCode;
use http::header;

use crate::auth::guard::AUDIT_TARGET;
use crate::operator::Operator;
use crate::operator::files;
use crate::operator::guard::Operator as Caller;
use crate::operator::guard::refusal;
use crate::operator::reload::Draft;
use crate::operator::reload::Prepared;
use crate::operator::reload::ReloadError;
use crate::operator::reload::Reloader;

/// The header a save names its reason in.
pub const REASON: &str = "ferrobridge-reason";

/// The longest reason a save may give, in characters.
const MAX_REASON: usize = 200;

/// The directory under `mapping_dir` a checked draft is staged in.
///
/// The `..` prefix keeps it out of every mapping walk.
const DRAFTS: &str = "..drafts";

/// The query of a draft check: the wire path the draft stands for.
#[derive(Debug, serde::Deserialize)]
pub struct CheckQuery {
    /// The wire path, such as `fhirconnect/condition.yml`.
    path: String,
}

/// `POST /operator/mappings/check?path=…`: the diagnostics of the running set
/// with the body standing in for the file at `path`.
pub async fn check(
    State(operator): State<Arc<Operator>>,
    Query(query): Query<CheckQuery>,
    body: Bytes,
) -> Response {
    let Some(mapping_dir) = operator.mapping_dir.clone() else {
        return unconfigured();
    };
    let relative = match files::relative(&query.path) {
        Ok((root, relative)) if files::Language::of_root(root).is_some() => relative,
        Ok(_) | Err(_) => return bad_path(),
    };
    let staged = mapping_dir
        .join(DRAFTS)
        .join(uuid::Uuid::new_v4().to_string())
        .join(&relative);
    let draft = Draft {
        wire: query.path,
        path: staged.clone(),
    };
    let checked = blocking(Arc::clone(&operator), move |operator| {
        write_durably(&draft.path, &body).map_err(Failure::Write)?;
        let outcome = operator.prepare(Some(&draft));
        let staging = draft.path.ancestors().nth(relative.components().count());
        if let Some(staging) = staging {
            remove_quietly(staging);
        }
        Ok((outcome, draft))
    })
    .await;
    match checked {
        Ok((Ok(prepared), draft)) => {
            axum::Json(operator.load_report(prepared.report(), Some(&draft))).into_response()
        }
        Ok((Err(ReloadError::Rejected(report)), draft)) => {
            axum::Json(operator.load_report(&report, Some(&draft))).into_response()
        }
        Ok((Err(error), _)) => unavailable(&error),
        Err(failure) => failure.into_response(),
    }
}

/// `PUT /operator/mappings/{path}`: saves the body as the console file at
/// `path` when it compiles with the set, and reloads.
pub async fn save(
    State(operator): State<Arc<Operator>>,
    Extension(Caller(caller)): Extension<Caller>,
    axum::extract::Path(wire): axum::extract::Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let subject = caller.subject().to_owned();
    let Some(mapping_dir) = operator.mapping_dir.clone() else {
        return unconfigured();
    };
    let Some((language, relative)) = files::relative(&wire)
        .ok()
        .and_then(|(root, relative)| Some((files::Language::of_root(root)?, relative)))
    else {
        return bad_path();
    };
    let target = language.console_directory(&mapping_dir).join(&relative);
    if !files::is_mapping(&target) {
        return bad_path();
    }
    let Some(reason) = reason(&headers) else {
        return refusal(
            StatusCode::BAD_REQUEST,
            None,
            "reason-required",
            "a save names its reason in the Ferrobridge-Reason header, one line of at most 200 characters",
        );
    };
    let precondition = Precondition::of(&headers);
    let new_digest = files::sha256(&body);
    let saved = blocking(Arc::clone(&operator), {
        let wire = wire.clone();
        move |operator| operator.save_blocking(&wire, &target, &body, &precondition)
    })
    .await;
    let (status, old_digest, response) = match saved {
        Ok(Saved::Done { old, report }) => (
            StatusCode::OK,
            old,
            (
                StatusCode::OK,
                [(header::ETAG, format!("\"{new_digest}\""))],
                axum::Json(report),
            )
                .into_response(),
        ),
        Ok(Saved::Rejected { old, report }) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            old,
            (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(report)).into_response(),
        ),
        Ok(Saved::Unreloaded { old, error }) => {
            (StatusCode::SERVICE_UNAVAILABLE, old, unavailable(&error))
        }
        Err(failure) => {
            let response = failure.into_response();
            (response.status(), None, response)
        }
    };
    tracing::info!(
        target: AUDIT_TARGET,
        subject = subject.as_str(),
        action = "save",
        file = wire.as_str(),
        old_sha256 = old_digest.as_deref().unwrap_or("-"),
        new_sha256 = new_digest.as_str(),
        reason = reason.as_str(),
        status = status.as_u16(),
        "operator"
    );
    response
}

/// `POST /operator/reload`: compiles the files on disk and swaps them in.
pub async fn reload(
    State(operator): State<Arc<Operator>>,
    Extension(Caller(caller)): Extension<Caller>,
) -> Response {
    let reloaded = blocking(Arc::clone(&operator), |operator| {
        Ok(operator
            .prepare(None)
            .map(|prepared| operator.apply(prepared)))
    })
    .await;
    let response = match reloaded {
        Ok(Ok(report)) => axum::Json(report).into_response(),
        Ok(Err(ReloadError::Rejected(report))) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            axum::Json(operator.load_report(&report, None)),
        )
            .into_response(),
        Ok(Err(error)) => unavailable(&error),
        Err(failure) => failure.into_response(),
    };
    tracing::info!(
        target: AUDIT_TARGET,
        subject = caller.subject(),
        action = "reload",
        status = response.status().as_u16(),
        "operator"
    );
    response
}

/// What a save's precondition headers ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Precondition {
    /// `If-Match` with this value: the file exists with this `ETag`, or any
    /// existing file for `*`.
    Match(String),
    /// `If-None-Match: *`: the file does not exist yet.
    Absent,
    /// Neither header.
    Missing,
}

impl Precondition {
    /// Returns the precondition `headers` carry.
    #[must_use]
    pub fn of(headers: &HeaderMap) -> Self {
        if let Some(value) = headers
            .get(header::IF_MATCH)
            .and_then(|value| value.to_str().ok())
        {
            return Self::Match(value.trim().to_owned());
        }
        let absent = headers
            .get(header::IF_NONE_MATCH)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.trim() == "*");
        if absent { Self::Absent } else { Self::Missing }
    }

    /// Whether a file whose digest is `current` (none when it does not exist)
    /// meets the precondition (RFC 9110 §13.1.1 and §13.1.2).
    #[must_use]
    pub fn holds(&self, current: Option<&str>) -> bool {
        match (self, current) {
            (Self::Match(tag), Some(digest)) => {
                tag == "*"
                    || tag
                        .split(',')
                        .any(|one| one.trim() == format!("\"{digest}\""))
            }
            (Self::Absent, None) => true,
            (Self::Match(_) | Self::Absent | Self::Missing, _) => false,
        }
    }
}

/// How a save ended, short of a failure to write.
#[derive(Debug)]
pub enum Saved {
    /// The file was written and the set reloaded.
    Done {
        /// The digest the file had before, when it existed.
        old: Option<String>,
        /// The report of the reloaded set.
        report: Option<ferrobridge_operator_api::operator::LoadReport>,
    },
    /// The draft did not compile; nothing was written.
    Rejected {
        /// The digest the file has.
        old: Option<String>,
        /// The report naming every diagnostic.
        report: ferrobridge_operator_api::operator::LoadReport,
    },
    /// The file was written and the reload did not run to the end, so the
    /// running set is the old one.
    Unreloaded {
        /// The digest the file had before.
        old: Option<String>,
        /// Why the reload stopped.
        error: ReloadError,
    },
}

/// Why a check or a save stopped before it compiled.
#[derive(Debug, thiserror::Error)]
pub enum Failure {
    /// The precondition headers are absent.
    #[error("a save names If-Match, or If-None-Match: * for a new file")]
    PreconditionRequired,
    /// The precondition does not hold.
    #[error("the file changed since the given ETag")]
    PreconditionFailed {
        /// The digest the file has.
        old: Option<String>,
    },
    /// A provisioned file holds the path.
    #[error("a provisioned file holds this path, and the console never writes one")]
    Provisioned,
    /// The draft or the saved file could not be written.
    #[error("the draft could not be written")]
    Write(#[source] std::io::Error),
    /// The current file could not be read.
    #[error("the current file could not be read")]
    Read(#[source] std::io::Error),
    /// The blocking task did not finish.
    #[error("the operator task did not finish")]
    Task(#[source] tokio::task::JoinError),
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        match self {
            Self::PreconditionRequired => refusal(
                StatusCode::PRECONDITION_REQUIRED,
                None,
                "precondition-required",
                "send If-Match with the file's ETag, or If-None-Match: * for a new file",
            ),
            Self::PreconditionFailed { .. } => refusal(
                StatusCode::PRECONDITION_FAILED,
                None,
                "precondition-failed",
                "the file changed since the ETag the save names",
            ),
            Self::Provisioned => refusal(
                StatusCode::CONFLICT,
                None,
                "provisioned",
                "a provisioned file holds this path, and the console never writes one",
            ),
            Self::Write(_) | Self::Read(_) | Self::Task(_) => {
                tracing::error!(
                    error = crate::facade::outcome::chain(&self),
                    "an operator save could not reach the file system"
                );
                refusal(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    None,
                    "unwritable",
                    "the mapping directory could not be written",
                )
            }
        }
    }
}

impl Operator {
    /// Checks and compiles the set on disk, `draft` standing in for one file.
    ///
    /// # Errors
    ///
    /// Returns what [`Reloader::prepare`] returns, and [`ReloadError::Read`]
    /// when a directory cannot be listed.
    pub fn prepare(&self, draft: Option<&Draft>) -> Result<Prepared, ReloadError> {
        let reloader = self
            .reloader
            .as_ref()
            .ok_or(ReloadError::Read(crate::walk::Error {
                path: PathBuf::new(),
                source: std::io::Error::other("the operator API has no reload configured"),
            }))?;
        let sources = Reloader::sources(&self.roots, draft)?;
        tokio::runtime::Handle::current().block_on(reloader.prepare(&sources))
    }

    /// Swaps `prepared` in and returns the report of the new set, or `None`
    /// when this surface reloads nothing.
    pub fn apply(
        &self,
        prepared: Prepared,
    ) -> Option<ferrobridge_operator_api::operator::LoadReport> {
        let snapshot = self.reloader.as_ref()?.apply(prepared);
        let report = self.load_report(snapshot.report(), None);
        self.replace(snapshot);
        Some(report)
    }

    /// Saves `body` at `target`, the console file `wire` names, when the
    /// precondition holds and the set compiles with it.
    fn save_blocking(
        &self,
        wire: &str,
        target: &Path,
        body: &[u8],
        precondition: &Precondition,
    ) -> Result<Saved, Failure> {
        if files::resolve(&self.provisioned_roots(), wire).is_ok() {
            return Err(Failure::Provisioned);
        }
        let old = match std::fs::read(target) {
            Ok(bytes) => Some(files::sha256(&bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(Failure::Read(error)),
        };
        if *precondition == Precondition::Missing {
            return Err(Failure::PreconditionRequired);
        }
        if !precondition.holds(old.as_deref()) {
            return Err(Failure::PreconditionFailed { old });
        }
        let staged = staging_path(target);
        write_durably(&staged, body).map_err(Failure::Write)?;
        let draft = Draft {
            wire: wire.to_owned(),
            path: staged.clone(),
        };
        match self.prepare(Some(&draft)) {
            Ok(_prepared) => {}
            Err(ReloadError::Rejected(report)) => {
                remove_quietly(&staged);
                return Ok(Saved::Rejected {
                    old,
                    report: self.load_report(&report, Some(&draft)),
                });
            }
            Err(error) => {
                remove_quietly(&staged);
                return Ok(Saved::Unreloaded { old, error });
            }
        }
        std::fs::rename(&staged, target).map_err(|error| {
            remove_quietly(&staged);
            Failure::Write(error)
        })?;
        sync_directory(target).map_err(Failure::Write)?;
        match self.prepare(None) {
            Ok(prepared) => Ok(Saved::Done {
                old,
                report: self.apply(prepared),
            }),
            Err(error) => Ok(Saved::Unreloaded { old, error }),
        }
    }
}

/// Returns the staging file a save of `target` writes first: a `..`-prefixed
/// sibling, so the rename stays in one directory and no walk reads it.
fn staging_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("mapping");
    target.with_file_name(format!("..{name}.{}.tmp", uuid::Uuid::new_v4()))
}

/// Writes `bytes` to `path`, creating its directories, and syncs the file.
fn write_durably(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Syncs the directory holding `path`, so a rename into it survives a crash.
fn sync_directory(path: &Path) -> std::io::Result<()> {
    match path.parent() {
        Some(parent) => std::fs::File::open(parent)?.sync_all(),
        None => Ok(()),
    }
}

/// Removes a staged file or directory, logging a failure.
fn remove_quietly(path: &Path) {
    let removed = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    if let Err(error) = removed {
        tracing::warn!(%error, "an operator draft could not be removed");
    }
}

/// Returns the reason `headers` name, or `None` for a missing or unusable one.
fn reason(headers: &HeaderMap) -> Option<String> {
    let reason = headers
        .get(REASON)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .unwrap_or_default();
    if reason.is_empty()
        || reason.chars().count() > MAX_REASON
        || reason.chars().any(char::is_control)
    {
        return None;
    }
    Some(reason.to_owned())
}

/// Runs `work` on a blocking thread, where the load path reads the file
/// system.
async fn blocking<T, F>(operator: Arc<Operator>, work: F) -> Result<T, Failure>
where
    F: FnOnce(&Operator) -> Result<T, Failure> + Send + 'static,
    T: Send + 'static,
{
    let _serialized = operator.saving.lock().await;
    let worker = Arc::clone(&operator);
    tokio::task::spawn_blocking(move || work(&worker))
        .await
        .map_err(Failure::Task)?
}

/// Returns the `409` of an operator API with no `mapping_dir`.
fn unconfigured() -> Response {
    refusal(
        StatusCode::CONFLICT,
        None,
        "not-configured",
        "no [operator] mapping_dir is configured, so the operator API checks and saves nothing",
    )
}

/// Returns the `400` of a path the console cannot save at.
fn bad_path() -> Response {
    refusal(
        StatusCode::BAD_REQUEST,
        None,
        "bad-path",
        "the path names no FHIRconnect or OMOCL mapping file the console can write",
    )
}

/// Returns the `503` of a reload an upstream or the file system stopped.
fn unavailable(error: &ReloadError) -> Response {
    tracing::error!(
        error = crate::facade::outcome::chain(error),
        "the mapping set could not be reloaded"
    );
    refusal(
        StatusCode::SERVICE_UNAVAILABLE,
        None,
        "unavailable",
        &crate::facade::outcome::chain(error),
    )
}
