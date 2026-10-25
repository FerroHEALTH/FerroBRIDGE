// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ported Data Quality Dashboard checks as an operator runs them: the
//! `dq check` job, the post-load step of `etl run`, and the two renderings of
//! a run.
//!
//! The JSON rendering keeps the Dashboard's own field names: a `CheckResults`
//! list with one object per check in the result columns of DQD v2.9.0
//! `R/recordResult.R` and the threshold fields of `R/evaluateThresholds.R`,
//! an `Overview` with the counts of `R/summarizeResults.R`, and the run's
//! `startTimestamp`, `endTimestamp` and `executionTimeSeconds`
//! (`R/executeDqChecks.R`). No specification governs the command surface:
//! our own design.

use std::collections::BTreeSet;
use std::time::Instant;

use omop_cdm::database::CdmPool;
use omop_cdm::quality::evaluate::{CheckResult, Summary};
use omop_cdm::quality::plan::{Plan, PlanError, PlanOptions};
use serde::Serialize;
use serde_json::{Value, json};

/// One run of the checks: every result, when it started and how long it
/// took.
#[derive(Debug)]
pub struct QualityRun {
    /// Every check's result, in plan order.
    pub results: Vec<CheckResult>,
    /// When the run started, as RFC 3339.
    pub started: String,
    /// When the run ended, as RFC 3339.
    pub ended: String,
    /// How long the run took, in seconds.
    pub seconds: f64,
}

/// The verdicts of a run, as the `etl run` report carries them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct QualityVerdicts {
    /// The checks run.
    pub total: usize,
    /// The checks that passed.
    pub passed: usize,
    /// The checks that failed their threshold.
    pub failed: usize,
    /// The checks that could not be evaluated.
    pub errors: usize,
    /// The checks that did not apply to the data.
    pub not_applicable: usize,
    /// The id of every check that failed or could not be evaluated, in plan
    /// order.
    pub failing: Vec<String>,
}

impl std::fmt::Display for QualityVerdicts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "quality checks: {} run, {} passed, {} failed, {} errors, {} not applicable",
            self.total, self.passed, self.failed, self.errors, self.not_applicable
        )?;
        for check in &self.failing {
            write!(f, "\nfailing check {check}")?;
        }
        Ok(())
    }
}

impl QualityRun {
    /// Returns whether every check passed or did not apply.
    #[must_use]
    pub fn clean(&self) -> bool {
        self.results
            .iter()
            .all(|result| !result.failed && !result.is_error)
    }

    /// Returns the verdicts the `etl run` report carries.
    #[must_use]
    pub fn verdicts(&self) -> QualityVerdicts {
        let mut verdicts = QualityVerdicts {
            total: self.results.len(),
            ..QualityVerdicts::default()
        };
        for result in &self.results {
            verdicts.passed += usize::from(result.passed);
            verdicts.failed += usize::from(result.failed);
            verdicts.errors += usize::from(result.is_error);
            verdicts.not_applicable += usize::from(result.not_applicable);
            if result.failed || result.is_error {
                verdicts.failing.push(result.check_id.clone());
            }
        }
        verdicts
    }

    /// Returns the run in the Dashboard's result fields.
    #[must_use]
    pub fn dashboard_json(&self) -> Value {
        let summary = Summary::new(&self.results);
        json!({
            "startTimestamp": self.started,
            "endTimestamp": self.ended,
            "executionTimeSeconds": self.seconds,
            "CheckResults": self.results.iter().map(result_json).collect::<Vec<_>>(),
            "Overview": overview_json(&summary),
        })
    }
}

/// A text summary: one line per failing check, then the counts.
impl std::fmt::Display for QualityRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for result in &self.results {
            let verdict = if result.is_error {
                "error"
            } else if result.failed {
                "failed"
            } else {
                continue;
            };
            let counts = result.counts.map_or_else(String::new, |counts| {
                format!(" ({} of {})", counts.violated, counts.denominator)
            });
            writeln!(f, "{verdict} {}{counts}", result.check_id)?;
        }
        let verdicts = self.verdicts();
        writeln!(
            f,
            "checks: {} run, {} passed, {} failed, {} errors, {} not applicable",
            verdicts.total,
            verdicts.passed,
            verdicts.failed,
            verdicts.errors,
            verdicts.not_applicable
        )?;
        Ok(())
    }
}

/// Returns one result in the Dashboard's result fields.
fn result_json(result: &CheckResult) -> Value {
    let counts = result.counts;
    json!({
        "numViolatedRows": counts.map(|counts| counts.violated),
        "pctViolatedRows": counts.map(omop_cdm::quality::evaluate::Counts::fraction),
        "numDenominatorRows": counts.map(|counts| counts.denominator),
        "executionTime": result.execution_time.map(|time| format!("{:.3} secs", time.as_secs_f64())),
        "queryText": result.query_text,
        "checkName": result.check_name.as_str(),
        "checkLevel": result.check_level.as_str(),
        "checkDescription": result.check_description,
        "cdmTableName": result.cdm_table_name,
        "cdmFieldName": result.cdm_field_name,
        "sqlFile": result.sql_file,
        "category": result.category.as_str(),
        "subcategory": result
            .subcategory
            .map(omop_cdm::quality::definitions::KahnSubcategory::as_str),
        "context": result.context.as_str(),
        "error": result.error.as_ref().map(|error| crate::chain(error)),
        "checkId": result.check_id,
        "failed": u8::from(result.failed),
        "passed": u8::from(result.passed),
        "isError": u8::from(result.is_error),
        "notApplicable": u8::from(result.not_applicable),
        "notApplicableReason": result.not_applicable_reason,
        "thresholdValue": result
            .threshold_value
            .map(omop_cdm::quality::definitions::Percentage::value),
        "notesValue": result.notes_value,
    })
}

/// Returns the overview in the Dashboard's field names.
fn overview_json(summary: &Summary) -> Value {
    json!({
        "countTotal": summary.count_total,
        "countPassed": summary.count_passed,
        "countErrorFailed": summary.count_error_failed,
        "countThresholdFailed": summary.count_threshold_failed,
        "countOverallFailed": summary.count_overall_failed,
        "percentPassed": summary.percent_passed(),
        "percentFailed": summary.percent_failed(),
        "countTotalPlausibility": summary.plausibility.total,
        "countTotalConformance": summary.conformance.total,
        "countTotalCompleteness": summary.completeness.total,
        "countFailedPlausibility": summary.plausibility.failed,
        "countFailedConformance": summary.conformance.failed,
        "countFailedCompleteness": summary.completeness.failed,
        "countPassedPlausibility": summary.plausibility.passed,
        "countPassedConformance": summary.conformance.passed,
        "countPassedCompleteness": summary.completeness.passed,
    })
}

/// Returns the plan over every CDM table, or over `tables` and the tables
/// derived from them when given, the vocabulary tables left out as the
/// Dashboard leaves them out by default.
///
/// # Errors
///
/// Returns [`PlanError`] when the generated settings name a column a table
/// lacks.
pub fn plan(tables: Option<&BTreeSet<String>>) -> Result<Plan, PlanError> {
    let mut options = PlanOptions::default();
    if let Some(tables) = tables {
        // NOTE: no specification governs this: our own design; the post-load step
        // checks what the run wrote and the person table every row refers to.
        let mut kept: BTreeSet<&str> = tables.iter().map(String::as_str).collect();
        kept.extend(["person", "observation_period", "condition_era", "drug_era"]);
        for table in &omop_cdm::generated::TABLES {
            if !kept.contains(table.name) {
                options.excluded_tables.insert(table.name.to_owned());
            }
        }
    }
    Plan::new(&options)
}

/// Runs `plan` on `pool`.
pub async fn run(pool: &CdmPool, plan: &Plan) -> QualityRun {
    let started = jiff::Timestamp::now();
    let clock = Instant::now();
    let results = omop_cdm::quality::run::run(pool, plan).await;
    QualityRun {
        results,
        started: started.to_string(),
        ended: jiff::Timestamp::now().to_string(),
        seconds: clock.elapsed().as_secs_f64(),
    }
}
