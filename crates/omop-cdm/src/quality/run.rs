// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Running a plan's checks on a CDM database.
//!
//! Each check's query runs on its own on the pool, so one check's failure
//! touches no other, and every check ends in a [`CheckResult`]: counts, a
//! missing table or column for a probe, or an error. No specification
//! governs how the queries are scheduled: our own design.

use std::time::Instant;

use sqlx::error::DatabaseError;

use crate::database::CdmPool;
use crate::quality::check::Check;
use crate::quality::evaluate::{self, CheckError, CheckResult, Counts, Outcome};
use crate::quality::plan::Plan;

/// PostgreSQL's `undefined_table` condition.
const UNDEFINED_TABLE: &str = "42P01";

/// PostgreSQL's `undefined_column` condition.
const UNDEFINED_COLUMN: &str = "42703";

/// Runs every check of `plan` on `pool`, in plan order.
///
/// A check whose query fails is an error result in the returned list; the run
/// itself does not fail.
pub async fn run(pool: &CdmPool, plan: &Plan) -> Vec<CheckResult> {
    let mut results = Vec::with_capacity(plan.checks().len());
    for check in plan.checks() {
        results.push(run_check(pool, check).await);
    }
    results
}

/// Runs one check on `pool` and evaluates what its query returns.
pub async fn run_check(pool: &CdmPool, check: &Check) -> CheckResult {
    let started = Instant::now();
    let row = sqlx::query_as::<_, (Option<i64>, Option<i64>)>(sqlx::AssertSqlSafe(check.sql()))
        .fetch_optional(pool.pool())
        .await;
    let elapsed = started.elapsed();
    let outcome = match row {
        Ok(Some((violated, denominator))) => match counts(violated, denominator) {
            Ok(counts) => Outcome::Counted(counts),
            Err(error) => Outcome::Failed(error),
        },
        Ok(None) => Outcome::Failed(CheckError::NoRow),
        Err(source) => {
            let absent = source
                .as_database_error()
                .and_then(DatabaseError::code)
                .is_some_and(|code| code == UNDEFINED_TABLE || code == UNDEFINED_COLUMN);
            let error = CheckError::Query {
                source: Box::new(source),
            };
            // NOTE: DQD v2.9.0 `R/evaluateThresholds.R` lines 156 to 162 read a
            // missing table or column from the message; SQLSTATE names it exactly.
            if absent {
                Outcome::Absent(error)
            } else {
                Outcome::Failed(error)
            }
        }
    };
    evaluate::evaluate(check, outcome, Some(elapsed))
}

/// Reads the two counts of a query's row, refusing a NULL or a negative one.
fn counts(violated: Option<i64>, denominator: Option<i64>) -> Result<Counts, CheckError> {
    Ok(Counts {
        violated: count("num_violated_rows", violated)?,
        denominator: count("num_denominator_rows", denominator)?,
    })
}

/// Reads one count.
fn count(column: &'static str, value: Option<i64>) -> Result<u64, CheckError> {
    let value = value.ok_or(CheckError::NullCount { column })?;
    u64::try_from(value).map_err(|source| CheckError::NegativeCount {
        column,
        value,
        source,
    })
}
