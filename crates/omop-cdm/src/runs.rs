// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The run history of the bridge schema: one row per ETL run, with when it
//! started and finished and the report it wrote, kept so that a server
//! beside the batch can show past runs.
//!
//! The report is a JSON document the caller serializes; this module stores
//! and returns its text and reads nothing inside it. A run is recorded once
//! it finished, by the connection of the writer that ran it, and read by a
//! pool of its own. No specification governs the history: our own design.

use sqlx::PgPool;
use sqlx::Row as _;

use crate::ddl::SchemaName;
use crate::writer::error::{Step, WriteError};
use crate::writer::input::RunId;

/// The statements that create the run table of the bridge schema `bridge`,
/// leaving one that already exists.
pub(crate) fn run_ddl(bridge: &SchemaName) -> [String; 2] {
    [
        format!(
            "CREATE TABLE IF NOT EXISTS {bridge}.run (
                run_id text PRIMARY KEY,
                started_at timestamptz NOT NULL,
                finished_at timestamptz NOT NULL,
                report jsonb NOT NULL)"
        ),
        format!("CREATE INDEX IF NOT EXISTS run_started ON {bridge}.run (started_at DESC)"),
    ]
}

/// One finished run, as the writer records it.
#[derive(Debug, Clone, Copy)]
pub struct RunRecord<'a> {
    /// The run.
    pub run_id: &'a RunId,
    /// When it started, RFC 3339.
    pub started: &'a str,
    /// When it finished, RFC 3339.
    pub finished: &'a str,
    /// The report it wrote, a JSON document.
    pub report: &'a str,
}

/// Records `run` in the run table of `bridge` over `client`, replacing a row
/// of the same run.
pub(crate) async fn record(
    client: &tokio_postgres::Client,
    bridge: &SchemaName,
    run: &RunRecord<'_>,
) -> Result<(), WriteError> {
    client
        .execute(
            &format!(
                "INSERT INTO {bridge}.run (run_id, started_at, finished_at, report)
                 VALUES ($1, $2::text::timestamptz, $3::text::timestamptz, $4::text::jsonb)
                 ON CONFLICT (run_id) DO UPDATE SET started_at = EXCLUDED.started_at,
                    finished_at = EXCLUDED.finished_at, report = EXCLUDED.report"
            ),
            &[
                &run.run_id.as_str(),
                &run.started,
                &run.finished,
                &run.report,
            ],
        )
        .await
        .map_err(|source| WriteError::Database {
            step: Step::RecordRun,
            source,
        })?;
    Ok(())
}

/// Why the run history could not be read.
#[derive(Debug, thiserror::Error)]
#[error("reading the run history of the bridge schema {bridge}")]
pub struct HistoryError {
    /// The bridge schema.
    bridge: SchemaName,
    /// What PostgreSQL or the pool reported.
    #[source]
    source: sqlx::Error,
}

/// Reads the run history of one bridge schema.
#[derive(Debug, Clone)]
pub struct History {
    /// The pool the reads go through.
    pool: PgPool,
    /// The bridge schema.
    bridge: SchemaName,
}

impl History {
    /// Returns the history of `bridge` read through `pool`.
    #[must_use]
    pub const fn new(pool: PgPool, bridge: SchemaName) -> Self {
        Self { pool, bridge }
    }

    /// Returns the reports of the newest `limit` runs, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`HistoryError`] when PostgreSQL refuses the read, as it does
    /// before the bridge schema holds the run table.
    pub async fn recent(&self, limit: u16) -> Result<Vec<String>, HistoryError> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT report::text FROM {}.run ORDER BY started_at DESC, run_id LIMIT $1",
            self.bridge
        )))
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(|source| self.refused(source))?;
        rows.iter()
            .map(|row| row.try_get::<String, _>(0))
            .collect::<Result<_, _>>()
            .map_err(|source| self.refused(source))
    }

    /// Returns the report of the run `run_id`, `None` when no run of that id
    /// is recorded.
    ///
    /// # Errors
    ///
    /// Returns [`HistoryError`] when PostgreSQL refuses the read.
    pub async fn run(&self, run_id: &str) -> Result<Option<String>, HistoryError> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT report::text FROM {}.run WHERE run_id = $1",
            self.bridge
        )))
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|source| self.refused(source))?;
        row.map(|row| row.try_get::<String, _>(0))
            .transpose()
            .map_err(|source| self.refused(source))
    }

    /// Returns the refusal of a read of this history.
    fn refused(&self, source: sqlx::Error) -> HistoryError {
        HistoryError {
            bridge: self.bridge.clone(),
            source,
        }
    }
}
