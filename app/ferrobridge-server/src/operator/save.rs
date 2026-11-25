// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Checking a draft, saving it and reloading the set.
//!
//! A draft is compiled in memory with every other file of the set. A save
//! holds the running revision for its whole change, compiles the running
//! revision's files with the draft in place, and records them as the new
//! running revision only when they compile for every lane; every replica then
//! swaps them in. A path a provisioned file holds is refused. The audit line
//! names the caller, the file, the two digests and the reason, never the
//! bytes. No specification governs the operator surface: our own design.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::Query;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use ferrohealth_smart::resource::Access;
use http::HeaderMap;
use http::StatusCode;
use http::header;

use crate::mappings::files;
use crate::mappings::files::Draft;
use crate::mappings::revisions::{Change, Console, Entry, Files, Recording};
use crate::operator::Operator;
use crate::operator::guard::AUDIT_TARGET;
use crate::operator::reload::Prepared;
use crate::operator::reload::ReloadError;
use crate::problem::{Kind, refuse};
use ferrobridge_operator_api::revisions::{Origin, Revision};

/// The header a save names its reason in.
pub const REASON: &str = "ferrobridge-reason";

/// The longest reason a save may give, in characters.
const MAX_REASON: usize = 200;

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
    if operator.reloader.is_none() {
        return Failure::Unconfigured.into_response();
    }
    match files::relative(&query.path) {
        Ok((root, _relative)) if files::Language::of_root(root).is_some() => {}
        Ok(_) | Err(_) => return Failure::BadEntry.into_response(),
    }
    let draft = Draft {
        wire: query.path,
        bytes: Arc::from(body.to_vec()),
    };
    let checked = blocking(Arc::clone(&operator), move |operator, thread| {
        let snapshot = operator.snapshot();
        Ok(operator.prepare(thread, snapshot.console().files(), Some(&draft)))
    })
    .await;
    match checked {
        Ok(Ok(prepared)) => axum::Json(operator.load_report(prepared.report())).into_response(),
        Ok(Err(ReloadError::Rejected(report))) => {
            axum::Json(operator.load_report(&report)).into_response()
        }
        Ok(Err(error)) => unavailable(&error),
        Err(failure) => failure.into_response(),
    }
}

/// `PUT /operator/mappings/{path}`: saves the body as the console file at
/// `path` when it compiles with the set, and reloads.
pub async fn save(
    State(operator): State<Arc<Operator>>,
    access: Access,
    axum::extract::Path(wire): axum::extract::Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let subject = subject_of(&access).to_owned();
    if operator.reloader.is_none() {
        return Failure::Unconfigured.into_response();
    }
    if files::Located::console(&wire).is_none() {
        return Failure::BadEntry.into_response();
    }
    let Some(reason) = reason(&headers) else {
        return refuse(
            Kind::ReasonRequired,
            "a save names its reason in the Ferrobridge-Reason header, one line of at most 200 characters",
        );
    };
    let precondition = Precondition::of(&headers);
    let new_digest = files::sha256(&body);
    let saved = blocking(Arc::clone(&operator), {
        let wire = wire.clone();
        let change = (subject.clone(), reason.clone());
        move |operator, thread| operator.save_blocking(thread, &wire, &body, &precondition, &change)
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

/// `POST /operator/reload`: compiles the provisioned files and the running
/// revision again and swaps them in.
pub async fn reload(State(operator): State<Arc<Operator>>, access: Access) -> Response {
    let reloaded = blocking(Arc::clone(&operator), |operator, thread| {
        let revisions = operator.revisions().ok_or(Failure::Unconfigured)?;
        let console = thread.wait(revisions.console()).map_err(Failure::Store)?;
        Ok(operator
            .prepare(thread, console.files(), None)
            .map(|prepared| operator.apply(prepared, console)))
    })
    .await;
    let response = match reloaded {
        Ok(Ok(report)) => axum::Json(report).into_response(),
        Ok(Err(ReloadError::Rejected(report))) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            axum::Json(operator.load_report(&report)),
        )
            .into_response(),
        Ok(Err(error)) => unavailable(&error),
        Err(failure) => failure.into_response(),
    };
    tracing::info!(
        target: AUDIT_TARGET,
        subject = subject_of(&access),
        action = "reload",
        status = response.status().as_u16(),
        "operator"
    );
    response
}

/// Returns who the change record names: the caller's subject, or `-` on an
/// open surface, which the configuration never mounts the operator API on.
pub(crate) fn subject_of(access: &Access) -> &str {
    access
        .caller()
        .map_or("-", ferrohealth_smart::resource::Caller::subject)
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
    /// The file was recorded in a running revision and the set reloaded.
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
    /// The check did not run to the end, so nothing was recorded and the
    /// running set is the old one.
    Unreloaded {
        /// The digest the file had before.
        old: Option<String>,
        /// Why the reload stopped.
        error: ReloadError,
    },
}

/// Why a set of console files did not compile into one that runs.
#[derive(Debug)]
pub enum Refused {
    /// An error diagnostic, with the report by wire path.
    Rejected(ferrobridge_operator_api::operator::LoadReport),
    /// The check could not run to the end.
    Unavailable(ReloadError),
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
    /// The blocking task did not finish.
    #[error("the operator task did not finish")]
    Task(#[source] tokio::task::JoinError),
    /// No `[state]` is configured, so nothing is saved.
    #[error("no [state] is configured")]
    Unconfigured,
    /// No revision has the id.
    #[error("no revision has this id")]
    UnknownRevision,
    /// A revision or an archive names a path that is no console mapping file.
    #[error("an entry names no console mapping file")]
    BadEntry,
    /// An import's body is no archive the bridge reads.
    #[error("the body is no mapping-set archive the bridge reads")]
    Archive(#[source] crate::operator::revisions::archive::ArchiveError),
    /// The revision store could not be read or written.
    #[error("the revision store could not be read or written")]
    Store(#[source] crate::mappings::revisions::RevisionError),
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        match self {
            Self::PreconditionRequired => refuse(
                Kind::PreconditionRequired,
                "send If-Match with the ETag of what the request replaces, or If-None-Match: * for a new file",
            ),
            Self::PreconditionFailed { .. } => refuse(
                Kind::PreconditionFailed,
                "what the request replaces changed since the ETag it names",
            ),
            Self::Provisioned => refuse(
                Kind::Provisioned,
                "a provisioned file holds this path, and the console never writes one",
            ),
            Self::Unconfigured => refuse(
                Kind::NotConfigured,
                "no [state] is configured, so the operator API checks and saves nothing",
            ),
            Self::UnknownRevision => refuse(Kind::NotFound, "no revision has this id"),
            Self::BadEntry => refuse(
                Kind::BadPath,
                "the path names no FHIRconnect or OMOCL mapping file the console can write",
            ),
            Self::Archive(_) => refuse(
                Kind::BadArchive,
                "the body is no mapping-set archive the bridge reads",
            ),
            Self::Task(_) | Self::Store(_) => {
                tracing::error!(
                    error = ferrohealth_server::run::chain(&self),
                    "an operator save could not reach the state store"
                );
                refuse(
                    Kind::Unwritable,
                    "the mapping revisions could not be read or written",
                )
            }
        }
    }
}

impl Operator {
    /// Checks and compiles the provisioned files with the console files
    /// `console`, `draft` standing in for one file.
    ///
    /// # Errors
    ///
    /// Returns what [`crate::operator::reload::Reloader::prepare`] returns,
    /// [`ReloadError::Read`] when a directory cannot be listed and
    /// [`ReloadError::Unconfigured`] when this surface reloads nothing.
    pub(crate) fn prepare(
        &self,
        thread: &OnBlockingThread,
        console: &Files,
        draft: Option<&Draft>,
    ) -> Result<Prepared, ReloadError> {
        let reloader = self.reloader.as_ref().ok_or(ReloadError::Unconfigured)?;
        let sources = files::sources(&self.roots, console, draft).map_err(ReloadError::Read)?;
        thread.wait(reloader.prepare(&sources))
    }

    /// Checks and compiles the provisioned files with the console files
    /// `console`, a rejection's report already shown by wire path.
    ///
    /// # Errors
    ///
    /// Returns [`Refused::Rejected`] with the report when a diagnostic is an
    /// error, and [`Refused::Unavailable`] when the check could not run.
    pub(crate) fn compile(
        &self,
        thread: &OnBlockingThread,
        console: &Files,
    ) -> Result<Prepared, Refused> {
        self.prepare(thread, console, None)
            .map_err(|error| match error {
                ReloadError::Rejected(report) => Refused::Rejected(self.load_report(&report)),
                error => Refused::Unavailable(error),
            })
    }

    /// Swaps `prepared`, compiled with `console`, in and returns the report
    /// of the new set, or `None` when this surface reloads nothing.
    pub fn apply(
        &self,
        prepared: Prepared,
        console: Console,
    ) -> Option<ferrobridge_operator_api::operator::LoadReport> {
        let snapshot = self
            .reloader
            .as_ref()?
            .apply(prepared)
            .with_console(console);
        let report = self.load_report(snapshot.report());
        self.replace(snapshot);
        Some(report)
    }

    /// Saves `body` as the console file `wire` names, in a new running
    /// revision, when the precondition holds and the set compiles with it.
    fn save_blocking(
        &self,
        thread: &OnBlockingThread,
        wire: &str,
        body: &[u8],
        precondition: &Precondition,
        (author, reason): &(String, String),
    ) -> Result<Saved, Failure> {
        if files::resolve(&self.roots, wire).is_ok() {
            return Err(Failure::Provisioned);
        }
        let revisions = self.revisions().ok_or(Failure::Unconfigured)?;
        let mut change = thread.wait(revisions.change()).map_err(Failure::Store)?;
        let mut contents = thread
            .wait(change.running_files())
            .map_err(Failure::Store)?;
        let old = contents.get(wire).map(|bytes| files::sha256(bytes));
        if *precondition == Precondition::Missing {
            return Err(Failure::PreconditionRequired);
        }
        if !precondition.holds(old.as_deref()) {
            return Err(Failure::PreconditionFailed { old });
        }
        contents.insert(wire.to_owned(), Arc::from(body));
        let prepared = match self.compile(thread, &contents) {
            Ok(prepared) => prepared,
            Err(Refused::Rejected(report)) => return Ok(Saved::Rejected { old, report }),
            Err(Refused::Unavailable(error)) => return Ok(Saved::Unreloaded { old, error }),
        };
        let entry = Entry {
            origin: Origin::Save,
            author: author.clone(),
            reason: reason.clone(),
            source: None,
        };
        let (_revision, report) = self.run(thread, (change, entry), prepared, contents)?;
        Ok(Saved::Done { old, report })
    }

    /// Records `contents` under `entry` as the running revision through
    /// `change`, then swaps `prepared`, compiled with them, in; returns the
    /// revision and the report of the new set.
    pub(crate) fn run(
        &self,
        thread: &OnBlockingThread,
        (change, entry): (Change, Entry),
        prepared: Prepared,
        contents: Files,
    ) -> Result<
        (
            Revision,
            Option<ferrobridge_operator_api::operator::LoadReport>,
        ),
        Failure,
    > {
        let revision = thread
            .wait(change.record(entry, &contents, Recording::Run))
            .map_err(Failure::Store)?;
        let report = self.apply(prepared, Console::new(Some(revision.id.clone()), contents));
        Ok((revision, report))
    }
}

/// Returns the reason `headers` name, or `None` for a missing or unusable one.
pub(crate) fn reason(headers: &HeaderMap) -> Option<String> {
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

/// Proof that the code holding it runs on a thread of the runtime's blocking
/// pool, the one place the load path may wait on the runtime.
///
/// Tokio's `Handle::block_on` panics on a thread that drives async tasks and
/// is safe on a `spawn_blocking` thread (tokio docs, `Handle::block_on`), so
/// the operator's prepare path takes this proof and only
/// [`on_blocking_thread`] mints it. No specification governs this: our own
/// design.
#[derive(Debug)]
pub(crate) struct OnBlockingThread(());

impl OnBlockingThread {
    /// Waits for `future` on this blocking thread.
    #[expect(
        clippy::unused_self,
        reason = "the receiver is the proof that this thread is a blocking one"
    )]
    pub(crate) fn wait<F: Future>(&self, future: F) -> F::Output {
        tokio::runtime::Handle::current().block_on(future)
    }
}

/// Runs `work` on a thread of the blocking pool, handing it the proof that
/// it runs there.
pub(crate) fn on_blocking_thread<T, F>(work: F) -> tokio::task::JoinHandle<T>
where
    F: FnOnce(&OnBlockingThread) -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || work(&OnBlockingThread(())))
}

/// Runs `work` on a blocking thread, where the load path reads the file
/// system, one operator change at a time.
pub(crate) async fn blocking<T, F>(operator: Arc<Operator>, work: F) -> Result<T, Failure>
where
    F: FnOnce(&Operator, &OnBlockingThread) -> Result<T, Failure> + Send + 'static,
    T: Send + 'static,
{
    let _serialized = operator.saving.lock().await;
    let worker = Arc::clone(&operator);
    on_blocking_thread(move |thread| work(&worker, thread))
        .await
        .map_err(Failure::Task)?
}

/// Returns the `503` of a reload an upstream or the file system stopped.
pub(crate) fn unavailable(error: &ReloadError) -> Response {
    tracing::error!(
        error = ferrohealth_server::run::chain(error),
        "the mapping set could not be reloaded"
    );
    refuse(
        Kind::ReloadUnavailable,
        "the mapping set could not be reloaded; the bridge log names the cause",
    )
}
