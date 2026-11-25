// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every replica runs the running revision.
//!
//! The replica listens on [`MAPPINGS`]; a notification, and every connect of
//! the listener, wakes one task that reads the running revision and, when it
//! is not the one this replica compiled, compiles it with the provisioned
//! files and swaps it in as a reload does. A revision that does not compile
//! here, or a store that does not answer, leaves the running set as it is and
//! is tried again after [`RETRY`]. No specification governs this: our own
//! design.

use std::sync::Arc;
use std::time::Duration;

use crate::operator::Operator;
use crate::operator::save::{Failure, blocking};
use crate::store::listen::{Listened, MAPPINGS, Wake};

/// The pause before a revision that did not swap in is tried again.
pub const RETRY: Duration = Duration::from_secs(5);

/// When an attempt compiles the running revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// Only when this replica runs another revision.
    Changed,
    /// Always, as the boot load does.
    Always,
}

/// How one attempt to run the running revision ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Converged {
    /// This replica already ran it.
    Current,
    /// It was compiled and swapped in.
    Swapped,
}

impl Operator {
    /// Compiles the running revision with the provisioned files and swaps
    /// it in, under [`When::Changed`] only when this replica runs another
    /// one.
    ///
    /// # Errors
    ///
    /// Returns [`Failure::Unconfigured`] when this surface saves nothing,
    /// [`Failure::Store`] when the store does not answer, and the reload's
    /// error when the revision does not compile here.
    pub async fn converge(self: &Arc<Self>, when: When) -> Result<Converged, Converging> {
        blocking(Arc::clone(self), move |operator, thread| {
            let revisions = operator.revisions().ok_or(Failure::Unconfigured)?;
            let console = thread.wait(revisions.console()).map_err(Failure::Store)?;
            if when == When::Changed
                && console.revision() == operator.snapshot().console().revision()
            {
                return Ok(Ok(Converged::Current));
            }
            Ok(operator
                .prepare(thread, console.files(), None)
                .map(|prepared| {
                    operator.apply(prepared, console);
                    Converged::Swapped
                }))
        })
        .await
        .map_err(Converging::Failure)?
        .map_err(Converging::Reload)
    }
}

/// Starts the task that runs each revision that becomes the running one.
///
/// The task is detached and ends with the runtime. Returns the state of
/// [`MAPPINGS`], through which the replica's `LISTEN` session wakes it;
/// `None` when `operator` saves no revisions.
pub fn start(operator: &Arc<Operator>) -> Option<Arc<Listened>> {
    operator.revisions()?;
    let wake = Arc::new(Wake::one());
    let listened = Arc::new(Listened::with_subscriber(MAPPINGS.name(), wake.clone()));
    tokio::spawn(converging(Arc::clone(operator), wake));
    Some(listened)
}

/// Why a revision did not swap in.
#[derive(Debug, thiserror::Error)]
pub enum Converging {
    /// The store or the task failed.
    #[error("the running revision could not be read")]
    Failure(#[source] Failure),
    /// The revision does not compile on this replica.
    #[error("the running revision did not compile on this replica")]
    Reload(#[source] crate::operator::reload::ReloadError),
}

/// The task that runs the running revision at each wake, trying again after
/// [`RETRY`] until it swaps in.
async fn converging(operator: Arc<Operator>, wake: Arc<Wake>) {
    let mut failed = false;
    loop {
        if failed {
            tokio::select! {
                () = wake.armed() => {}
                () = tokio::time::sleep(RETRY) => {}
            }
        } else {
            wake.armed().await;
        }
        match operator.converge(When::Changed).await {
            Ok(Converged::Current) => failed = false,
            Ok(Converged::Swapped) => {
                failed = false;
                tracing::info!(
                    revision = operator.snapshot().console().revision().unwrap_or("-"),
                    "the running mapping revision is swapped in"
                );
            }
            Err(error) => {
                failed = true;
                tracing::error!(
                    error = ferrohealth_server::run::chain(&error),
                    "the running mapping revision did not swap in"
                );
            }
        }
    }
}
