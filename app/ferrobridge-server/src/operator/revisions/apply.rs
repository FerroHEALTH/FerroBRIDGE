// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Applying a revision.
//!
//! The revision's files are compiled with the provisioned ones while the
//! change holds the running revision, and only a set that compiles for every
//! lane is recorded as running, then swapped in here and on every other
//! replica. A rollback is the apply of an earlier revision. No specification
//! governs this: our own design.

use ferrobridge_operator_api::operator::LoadReport;
use ferrobridge_operator_api::revisions::{Origin, Revision};

use crate::mappings::revisions::Entry;
use crate::operator::Operator;
use crate::operator::reload::ReloadError;
use crate::operator::save::{Failure, OnBlockingThread, Precondition, Uncompiled};

/// How an apply ended, short of a failure to reach the store.
#[derive(Debug)]
pub enum Applied {
    /// The revision's files run, here and on every replica.
    Done {
        /// The revision the apply recorded, now running.
        revision: Revision,
        /// The report of the reloaded set.
        report: Option<LoadReport>,
    },
    /// The set does not compile with the revision's files; nothing changed.
    Rejected {
        /// The report naming every diagnostic.
        report: LoadReport,
    },
    /// The check could not run to the end; nothing changed.
    Unreloaded {
        /// Why it stopped.
        error: ReloadError,
    },
}

impl Operator {
    /// Applies the revision `id` when `precondition` names the running
    /// revision and the set compiles with its files, recording the apply
    /// under `author` and `reason`; returns the running revision's id before
    /// the apply beside the outcome.
    pub(crate) fn apply_blocking(
        &self,
        thread: &OnBlockingThread,
        id: &str,
        precondition: &Precondition,
        author: &str,
        reason: &str,
    ) -> Result<(Option<String>, Applied), Failure> {
        let revisions = self.revisions().ok_or(Failure::Unconfigured)?;
        let mut change = thread.wait(revisions.change()).map_err(Failure::Store)?;
        let running = change.running().map(|revision| revision.id.clone());
        if *precondition == Precondition::Missing {
            return Err(Failure::PreconditionRequired);
        }
        if !precondition.holds(running.as_deref()) {
            return Err(Failure::PreconditionFailed { old: running });
        }
        let target = thread
            .wait(change.get(id))
            .map_err(Failure::Store)?
            .ok_or(Failure::UnknownRevision)?;
        let contents = thread
            .wait(change.contents(&target))
            .map_err(Failure::Store)?;
        let prepared = match self.compile(thread, &contents) {
            Ok(prepared) => prepared,
            Err(Uncompiled::Rejected(report)) => {
                return Ok((running, Applied::Rejected { report }));
            }
            Err(Uncompiled::Unavailable(error)) => {
                return Ok((running, Applied::Unreloaded { error }));
            }
        };
        let entry = Entry {
            origin: Origin::Apply,
            author: author.to_owned(),
            reason: reason.to_owned(),
            source: Some(target.id),
        };
        let (revision, report) = self.run(thread, (change, entry), prepared, contents)?;
        Ok((running, Applied::Done { revision, report }))
    }
}
