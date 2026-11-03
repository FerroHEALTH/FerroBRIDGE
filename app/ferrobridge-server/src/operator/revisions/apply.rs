// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Applying a revision.
//!
//! The revision's files are staged in a tree of their own, the set is
//! compiled with that tree standing in for the console's, and only a set
//! that compiles for every lane swapped in, each console directory replaced
//! by one rename. A rollback is the apply of an earlier revision. No
//! specification governs this: our own design.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use ferrobridge_operator_api::operator::LoadReport;
use ferrobridge_operator_api::revisions::{Origin, Revision};

use crate::operator::Operator;
use crate::operator::files;
use crate::operator::files::{Language, Root};
use crate::operator::reload::{Prepared, ReloadError};
use crate::operator::revisions::{BRIDGE, Entry};
use crate::operator::save::{Failure, Precondition};

/// How an apply ended, short of a failure to reach the disk.
#[derive(Debug)]
pub enum Applied {
    /// The revision's files replaced the console's and the set reloaded.
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
    /// The check or the reload could not run to the end.
    Unreloaded {
        /// Why it stopped.
        error: ReloadError,
    },
}

/// A tree of console files staged under `mapping_dir/..drafts`, removed when
/// it is dropped.
#[derive(Debug)]
pub struct Staged {
    /// The tree, holding `fhirconnect/` and `omocl/`.
    directory: PathBuf,
}

impl Staged {
    /// Writes `contents`, by wire path, into a fresh tree under
    /// `mapping_dir`.
    ///
    /// # Errors
    ///
    /// Returns [`Failure::BadEntry`] for a wire path that names no console
    /// file and [`Failure::Write`] when the tree cannot be written.
    pub fn write(
        mapping_dir: &Path,
        contents: &BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, Failure> {
        let staged = Self {
            directory: mapping_dir
                .join(crate::operator::save::DRAFTS)
                .join(uuid::Uuid::new_v4().to_string()),
        };
        std::fs::create_dir_all(&staged.directory).map_err(Failure::Write)?;
        for (wire, bytes) in contents {
            let Ok((root, relative)) = files::relative(wire) else {
                return Err(Failure::BadEntry);
            };
            let Some(language) = Language::of_root(root) else {
                return Err(Failure::BadEntry);
            };
            let path = language.console_directory(&staged.directory).join(relative);
            if !files::is_mapping(&path) {
                return Err(Failure::BadEntry);
            }
            crate::operator::save::write_durably(&path, bytes).map_err(Failure::Write)?;
        }
        Ok(staged)
    }

    /// Returns the provisioned roots of `operator` with this tree standing
    /// in for the console's.
    #[must_use]
    pub fn roots(&self, operator: &Operator) -> Vec<Root> {
        let mut roots = operator.provisioned_roots();
        for language in [Language::Fhirconnect, Language::Omocl] {
            roots.push(Root::console(
                language,
                language.console_directory(&self.directory),
            ));
        }
        roots
    }

    /// Replaces each console directory under `mapping_dir` by this tree's,
    /// one rename each.
    fn swap_into(&self, mapping_dir: &Path) -> std::io::Result<()> {
        for language in [Language::Fhirconnect, Language::Omocl] {
            let live = language.console_directory(mapping_dir);
            let incoming = language.console_directory(&self.directory);
            let outgoing = self.directory.join(format!("..old-{}", language.root()));
            if live.exists() {
                std::fs::rename(&live, &outgoing)?;
            }
            if incoming.exists()
                && let Err(error) = std::fs::rename(&incoming, &live)
            {
                if outgoing.exists()
                    && let Err(back) = std::fs::rename(&outgoing, &live)
                {
                    tracing::error!(%back, "a console directory could not be put back");
                }
                return Err(error);
            }
        }
        std::fs::File::open(mapping_dir)?.sync_all()
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.directory)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "a staged revision could not be removed");
        }
    }
}

impl Operator {
    /// Checks and compiles the set with `staged` standing in for the console
    /// files, answering the report by the staged wire paths.
    ///
    /// # Errors
    ///
    /// Returns what [`Operator::prepare_over`] returns, the rejection's
    /// report already shown.
    pub fn check_staged(&self, staged: &Staged) -> Result<Prepared, Checked> {
        let roots = staged.roots(self);
        match self.prepare_over(&roots, None) {
            Ok(prepared) => Ok(prepared),
            Err(ReloadError::Rejected(report)) => Err(Checked::Rejected(
                crate::operator::load_report_over(&report, &roots, None),
            )),
            Err(error) => Err(Checked::Unavailable(error)),
        }
    }

    /// Applies the revision `id` when `precondition` names the running
    /// revision and the set compiles with its files, recording the apply
    /// under `author` and `reason`; returns the running revision's id before
    /// the apply beside the outcome.
    pub(crate) fn apply_blocking(
        &self,
        id: &str,
        precondition: &Precondition,
        author: &str,
        reason: &str,
    ) -> Result<(Option<String>, Applied), Failure> {
        let (Some(mapping_dir), Some(store)) =
            (self.mapping_dir.as_deref(), self.revisions.as_ref())
        else {
            return Err(Failure::Unconfigured);
        };
        self.settle(Entry {
            origin: Origin::Reload,
            author: String::from(BRIDGE),
            reason: String::from("the console files changed on disk"),
            applied: true,
            source: None,
        })
        .map_err(Failure::Store)?;
        let running = store
            .running()
            .map_err(Failure::Store)?
            .map(|revision| revision.id);
        if *precondition == Precondition::Missing {
            return Err(Failure::PreconditionRequired);
        }
        if !precondition.holds(running.as_deref()) {
            return Err(Failure::PreconditionFailed { old: running });
        }
        let target = store
            .get(id)
            .map_err(Failure::Store)?
            .ok_or(Failure::UnknownRevision)?;
        let contents = store.contents(&target).map_err(Failure::Store)?;
        let staged = Staged::write(mapping_dir, &contents)?;
        match self.check_staged(&staged) {
            Ok(_prepared) => {}
            Err(Checked::Rejected(report)) => return Ok((running, Applied::Rejected { report })),
            Err(Checked::Unavailable(error)) => {
                return Ok((running, Applied::Unreloaded { error }));
            }
        }
        staged.swap_into(mapping_dir).map_err(Failure::Write)?;
        drop(staged);
        let prepared = match self.prepare(None) {
            Ok(prepared) => prepared,
            Err(error) => return Ok((running, Applied::Unreloaded { error })),
        };
        let report = self.apply(prepared);
        let revision = store
            .record(
                Entry {
                    origin: Origin::Apply,
                    author: author.to_owned(),
                    reason: reason.to_owned(),
                    applied: true,
                    source: Some(target.id),
                },
                &contents,
            )
            .map_err(Failure::Store)?;
        Ok((running, Applied::Done { revision, report }))
    }
}

/// Why a staged tree did not compile into a set.
#[derive(Debug)]
pub enum Checked {
    /// An error diagnostic, with the report by the staged wire paths.
    Rejected(LoadReport),
    /// The check could not run to the end.
    Unavailable(ReloadError),
}
