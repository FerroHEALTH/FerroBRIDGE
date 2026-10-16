// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The compositions an earlier run committed that this run's query did not
//! answer, checked with the CDR and removed when it reports them deleted.
//!
//! A logical deletion is a new version whose data is Void (openEHR RM Common
//! 1.1.0 §6.2.5), so a `LATEST_VERSION` query stops answering a deleted
//! composition and the run never sees it go. After the composition loop the
//! run reads every watermark the query did not answer, asks the CDR for the
//! composition's latest version over ITS-REST 1.1.0 `composition_get` with
//! the versioned object id, and acts on the answer: `204` is the deletion
//! (`204_deleted_at_time`), `200` an extant composition the query left out,
//! and `404` a composition the CDR does not know. Only the `204` removes
//! rows; a call that reaches no documented answer ends the run, so "could
//! not check" never reads as "deleted". No specification governs the sweep
//! or the removal: our own design.

use crate::cdr::CdrClient;
use crate::cdr::error::CdrError;
use crate::cdr::ids::{EhrId, IdError};
use crate::etl::RunError;
use crate::etl::report::RunReport;
use futures_util::StreamExt;
use omop_cdm::writer::CdmWriter;
use omop_cdm::writer::removal::{Committed, Removal};
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::uid_based_id::UidBasedId;
use openehr_its::rest::generated::ehr::client::CompositionGetOutcome;
use std::collections::BTreeSet;

/// How many composition checks one run keeps in flight at once.
// NOTE: no specification governs this: our own design; a bound keeps a sweep over a
// large CDM from opening one CDR call per composition at once.
const CHECKS_IN_FLIGHT: usize = 8;

/// What a run does with a composition the CDR reports deleted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeletionPolicy {
    /// Removes the composition's rows, links and watermark.
    #[default]
    Remove,
    /// Lists the composition in the run report and keeps its rows.
    Report,
}

/// What the CDR answers for the latest version of one composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// `204`: the latest version is a deletion.
    Deleted,
    /// `200`: the latest version holds a composition.
    Extant,
    /// `404`: the CDR knows no such EHR or composition.
    Unknown,
}

/// What the run does with one composition its query did not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Removes its rows.
    Remove,
    /// Lists it as deleted and keeps its rows.
    ReportDeleted,
    /// Keeps its rows: the CDR still holds it.
    KeepExtant,
    /// Keeps its rows and lists it: the CDR does not confirm a deletion.
    KeepUnknown,
}

/// Returns what `policy` does with a composition the CDR answered `presence`
/// for.
///
/// Only a confirmed deletion removes rows: a `404` names no deletion
/// (`404_unknown_ehr_id_or_no_version_at_time`), so it keeps them.
#[must_use]
pub fn decide(policy: DeletionPolicy, presence: Presence) -> Action {
    match (presence, policy) {
        (Presence::Deleted, DeletionPolicy::Remove) => Action::Remove,
        (Presence::Deleted, DeletionPolicy::Report) => Action::ReportDeleted,
        (Presence::Extant, _) => Action::KeepExtant,
        (Presence::Unknown, _) => Action::KeepUnknown,
    }
}

/// Why the CDR could not say whether a composition is deleted.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CheckError {
    /// The side table's EHR id cannot travel in the request path.
    #[error("the EHR id the side table holds cannot name a CDR resource")]
    EhrId(#[source] IdError),
    /// The call reached no documented answer.
    #[error("the CDR did not answer the composition read")]
    Cdr(#[source] CdrError),
}

/// Asks the CDR whether the latest version of the composition `container`
/// holds in the EHR `ehr_id` is a deletion.
///
/// The read names the version container with no `version_at_time`, which
/// answers its latest version (`ehr-codegen.openapi.yaml`,
/// `composition_get`).
///
/// # Errors
///
/// Returns [`CdrError`] when the call reaches no documented answer, with the
/// upstream status when the CDR sent one.
pub async fn presence(
    cdr: &CdrClient,
    ehr_id: &EhrId,
    container: &HierObjectId,
) -> Result<Presence, CdrError> {
    let answered = cdr
        .composition(ehr_id, &UidBasedId::HierObjectId(container.clone()), None)
        .await?;
    Ok(match answered.outcome {
        CompositionGetOutcome::Ok { .. } => Presence::Extant,
        CompositionGetOutcome::NoContent => Presence::Deleted,
        CompositionGetOutcome::NotFound { .. } => Presence::Unknown,
    })
}

/// Asks the CDR whether the composition `committed` names is deleted.
async fn check(cdr: &CdrClient, committed: &Committed) -> Result<Presence, CheckError> {
    let ehr_id = EhrId::new(committed.ehr_id().value()).map_err(CheckError::EhrId)?;
    presence(cdr, &ehr_id, committed.versioned_object_uid())
        .await
        .map_err(CheckError::Cdr)
}

/// What the sweep reads and acts on.
pub(super) struct Sweep<'s> {
    /// The policy for a confirmed deletion.
    pub(super) policy: DeletionPolicy,
    /// How many watermarks one batch reads.
    pub(super) batch: u32,
    /// The versioned compositions this run's query answered.
    pub(super) seen: &'s BTreeSet<String>,
}

/// Checks every composition a watermark names and this run did not read,
/// batch by batch, and acts on each answer.
///
/// # Errors
///
/// Returns [`RunError::Deletion`] when the CDR cannot say whether a
/// composition is deleted, and [`RunError::Database`] when the side table
/// cannot be read or a removal is refused; removals committed before the
/// error stay committed, each one whole.
pub(super) async fn sweep(
    sweep: &Sweep<'_>,
    cdr: &CdrClient,
    writer: &mut CdmWriter,
    report: &mut RunReport,
) -> Result<(), RunError> {
    let mut after = None;
    loop {
        let batch = writer
            .watermarks_after(after.as_ref(), sweep.batch)
            .await
            .map_err(RunError::Database)?;
        let Some(last) = batch.last() else {
            return Ok(());
        };
        after = Some(last.versioned_object_uid().clone());
        let short = u32::try_from(batch.len()).is_ok_and(|read| read < sweep.batch);
        let candidates: Vec<Committed> = batch
            .into_iter()
            .filter(|committed| {
                !sweep
                    .seen
                    .contains(committed.versioned_object_uid().value())
            })
            .collect();
        let answers: Vec<Result<Presence, CheckError>> =
            futures_util::stream::iter(candidates.iter().map(|committed| check(cdr, committed)))
                .buffered(CHECKS_IN_FLIGHT)
                .collect()
                .await;
        for (committed, answer) in candidates.iter().zip(answers) {
            let composition = committed.versioned_object_uid().value().to_owned();
            let presence = answer.map_err(|source| RunError::Deletion {
                composition: composition.clone(),
                source: Box::new(source),
            })?;
            report.deletions.checked = report.deletions.checked.saturating_add(1);
            match decide(sweep.policy, presence) {
                Action::Remove => match writer.remove(committed).await {
                    Ok(Removal::Removed(rows)) => report.removed(composition, &rows),
                    Ok(Removal::Superseded) => {
                        report.deletions.superseded = report.deletions.superseded.saturating_add(1);
                    }
                    Err(error) => return Err(RunError::Database(error)),
                },
                Action::ReportDeleted => report.deletions.kept.push(composition),
                Action::KeepExtant => {
                    report.deletions.extant = report.deletions.extant.saturating_add(1);
                }
                Action::KeepUnknown => report.deletions.unknown.push(composition),
            }
        }
        if short {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, DeletionPolicy, Presence, decide};

    #[test]
    fn a_confirmed_deletion_removes_the_rows_under_the_remove_policy() {
        assert_eq!(
            Action::Remove,
            decide(DeletionPolicy::Remove, Presence::Deleted)
        );
    }

    #[test]
    fn a_confirmed_deletion_is_only_reported_under_the_report_policy() {
        assert_eq!(
            Action::ReportDeleted,
            decide(DeletionPolicy::Report, Presence::Deleted)
        );
    }

    #[test]
    fn an_extant_composition_keeps_its_rows_under_either_policy() {
        for policy in [DeletionPolicy::Remove, DeletionPolicy::Report] {
            assert_eq!(Action::KeepExtant, decide(policy, Presence::Extant));
        }
    }

    #[test]
    fn a_composition_the_cdr_does_not_know_keeps_its_rows_under_either_policy() {
        for policy in [DeletionPolicy::Remove, DeletionPolicy::Report] {
            assert_eq!(Action::KeepUnknown, decide(policy, Presence::Unknown));
        }
    }

    #[test]
    fn the_default_policy_removes() {
        assert_eq!(DeletionPolicy::Remove, DeletionPolicy::default());
    }
}
