// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The run history: each finished `etl run` and `etl fhir` kept in the bridge
//! schema, and read back for the operator API.
//!
//! A job records its run once the load and its checks finished, as the
//! operator document [`Run`] in the run table of `[cdm] bridge_schema`
//! (`omop_cdm::runs`). `serve` reads the table through a lazy pool of its
//! own, opened on the first read over the TLS the `[cdm]` section settles,
//! so a CDM database that is down stops only the history's routes. No
//! specification governs the history: our own design.

use ferrobridge_operator_api::etl::{
    Compositions, Deletions, Refusal, Run, RunKind, RunSummary, Runs,
};
use omop_cdm::runs::{History, HistoryError, RunRecord};
use omop_cdm::writer::CdmWriter;
use omop_cdm::writer::error::WriteError;
use omop_cdm::writer::input::RunId;

use crate::config::CdmSettings;
use crate::etl::report::RunReport;

/// How many runs the listing names.
pub const LISTED: u16 = 50;

/// Why a run could not be recorded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RecordError {
    /// The run identifier is empty.
    #[error("the run report names no run")]
    RunId(#[source] omop_cdm::graph::key::EmptyIdentifier),
    /// The report does not serialize.
    #[error("serializing the run report")]
    Serialize(#[source] serde_json::Error),
    /// The writer's connection could not be opened.
    #[error("connecting to the CDM database to record the run")]
    Connect(#[source] WriteError),
    /// PostgreSQL refused the record.
    #[error("recording the run in the bridge schema")]
    Write(#[source] WriteError),
}

/// Why the history could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadError {
    /// PostgreSQL refused the read or could not be reached.
    #[error("reading the run history")]
    Database(#[source] HistoryError),
    /// A recorded report is no run document.
    #[error("the run history holds a report that is no run document")]
    Document(#[source] serde_json::Error),
}

/// Returns the operator document of `report`, a run of `kind` from
/// `started` to `finished`.
#[must_use]
pub fn document(
    report: &RunReport,
    kind: RunKind,
    started: jiff::Timestamp,
    finished: jiff::Timestamp,
) -> Run {
    let compositions = &report.compositions;
    let deletions = &report.deletions;
    Run {
        id: report.run_id.clone(),
        kind,
        started: started.to_string(),
        finished: finished.to_string(),
        compositions: Compositions {
            read: compositions.read,
            committed: compositions.committed,
            skipped: compositions.skipped,
            refused: compositions.refused,
        },
        rows: report.rows.clone(),
        concept_zero: report.concept_zero,
        refusals: report
            .refusals
            .iter()
            .map(|refusal| Refusal {
                composition: refusal.composition.clone(),
                mapping: refusal.mapping.clone(),
                table: refusal.table.clone(),
                column: refusal.column.clone(),
                element: refusal.element.clone(),
                reason: refusal.reason.clone(),
            })
            .collect(),
        deletions: Deletions {
            checked: deletions.checked,
            extant: deletions.extant,
            removed: deletions.removed.clone(),
            kept: deletions.kept.clone(),
            unknown: deletions.unknown.clone(),
            rows: deletions.rows.clone(),
        },
    }
}

/// Returns the listing entry of `run`.
#[must_use]
pub fn summary(run: &Run) -> RunSummary {
    let count = |items: usize| u64::try_from(items).unwrap_or(u64::MAX);
    RunSummary {
        id: run.id.clone(),
        kind: run.kind,
        started: run.started.clone(),
        finished: run.finished.clone(),
        compositions: run.compositions,
        rows: run
            .rows
            .values()
            .fold(0_u64, |sum, rows| sum.saturating_add(*rows)),
        refusals: count(run.refusals.len()),
        removed: count(run.deletions.removed.len()),
    }
}

/// Records `run` in the run table of the bridge schema `writer` writes.
///
/// # Errors
///
/// Returns [`RecordError`] when the run names no identifier, does not
/// serialize, or PostgreSQL refuses the record, as it does before
/// `ferrobridge cdm init` created the run table.
pub async fn record(writer: &CdmWriter, run: &Run) -> Result<(), RecordError> {
    let run_id = RunId::new(run.id.clone()).map_err(RecordError::RunId)?;
    let report = serde_json::to_string(run).map_err(RecordError::Serialize)?;
    writer
        .record_run(&RunRecord {
            run_id: &run_id,
            started: &run.started,
            finished: &run.finished,
            report: &report,
        })
        .await
        .map_err(RecordError::Write)
}

/// The run history `serve` reads.
#[derive(Debug, Clone)]
pub struct Store {
    /// The history of the bridge schema.
    history: History,
}

impl Store {
    /// Returns the history of the bridge schema of `cdm`.
    ///
    /// Nothing is opened until the first read.
    ///
    /// # Errors
    ///
    /// Returns the [`omop_cdm::connection::ConnectionError`] of
    /// `CdmConnection::pool_options`, when the pool's TLS relay cannot be
    /// started.
    pub fn new(cdm: &CdmSettings) -> Result<Self, omop_cdm::connection::ConnectionError> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .min_connections(0)
            .acquire_timeout(ferrohealth_health::registry::CHECK_TIMEOUT)
            .connect_lazy_with(cdm.connection.pool_options()?);
        Ok(Self {
            history: History::new(pool, cdm.bridge_schema.clone()),
        })
    }

    /// Returns the newest [`LISTED`] runs, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`ReadError`] when the history cannot be read or holds a
    /// report that is no run document.
    pub async fn runs(&self) -> Result<Runs, ReadError> {
        let reports = self
            .history
            .recent(LISTED)
            .await
            .map_err(ReadError::Database)?;
        let runs = reports
            .iter()
            .map(|report| {
                serde_json::from_str(report)
                    .map(|run| summary(&run))
                    .map_err(ReadError::Document)
            })
            .collect::<Result<_, _>>()?;
        Ok(Runs { runs })
    }

    /// Returns the run `run_id`, `None` when the history holds none of that
    /// id.
    ///
    /// # Errors
    ///
    /// As [`Store::runs`].
    pub async fn run(&self, run_id: &str) -> Result<Option<Run>, ReadError> {
        self.history
            .run(run_id)
            .await
            .map_err(ReadError::Database)?
            .as_deref()
            .map(|report| serde_json::from_str(report).map_err(ReadError::Document))
            .transpose()
    }
}
