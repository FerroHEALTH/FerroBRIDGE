// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! From what a check's query returned to the Dashboard's verdict.
//!
//! [`evaluate`] applies the Dashboard's threshold rule
//! (DQD v2.9.0 `R/evaluateThresholds.R` lines 156 to 180) to one check's
//! [`Outcome`] and records a [`CheckResult`] with the Dashboard's result fields
//! (`R/recordResult.R` lines 51 to 78). [`Summary`] counts a run the way
//! `R/summarizeResults.R` does, except that a pass is a row whose `passed` is
//! true.
//!
//! A check never passes without counts: a query that fails, or that returns no
//! row or a NULL count, is an error result, and an error result is never
//! passed.

use std::num::TryFromIntError;
use std::time::Duration;

use crate::quality::check::{Check, CheckName};
use crate::quality::definitions::{
    CheckLevel, KahnCategory, KahnContext, KahnSubcategory, Percentage,
};

/// The two counts a check's query returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// The rows that violate the check (`numViolatedRows`).
    pub violated: u64,
    /// The rows the check is measured over (`numDenominatorRows`).
    pub denominator: u64,
}

impl Counts {
    /// Returns the fraction of violating rows (`pctViolatedRows`), 0 when the
    /// denominator is 0 as in every template.
    #[must_use]
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "the fraction is a report value; the verdict is computed exactly in integers"
    )]
    pub fn fraction(self) -> f64 {
        if self.denominator == 0 {
            0.0
        } else {
            self.violated as f64 / self.denominator as f64
        }
    }

    /// Returns whether the counts exceed `threshold` under the Dashboard's
    /// rule.
    ///
    /// With no threshold, or a threshold of 0, any violating row fails the
    /// check; otherwise the check fails when the percentage of violating rows
    /// is strictly above the threshold (DQD v2.9.0 `R/evaluateThresholds.R`
    /// lines 167 to 174). The comparison is exact: `violated * 100` against
    /// `threshold * denominator`.
    #[must_use]
    pub fn exceed(self, threshold: Option<Percentage>) -> bool {
        match threshold.map(Percentage::value) {
            None | Some(0) => self.violated > 0,
            // NOTE: DQD v2.9.0 `inst/sql/sql_server/*.sql` report 0 when the
            // denominator is 0, and 0 is above no threshold.
            Some(_) if self.denominator == 0 => false,
            Some(threshold) => {
                u128::from(self.violated) * 100
                    > u128::from(threshold) * u128::from(self.denominator)
            }
        }
    }
}

/// Why a check produced no counts.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CheckError {
    /// The database refused or failed the query.
    #[cfg(feature = "database")]
    #[error("the database refused or failed the check's query")]
    Query {
        /// The database's error.
        #[source]
        source: Box<sqlx::Error>,
    },
    /// The query returned no row.
    #[error("the check's query returned no row")]
    NoRow,
    /// The query returned NULL for a count.
    #[error("the check's query returned NULL for {column}")]
    NullCount {
        /// The count's column.
        column: &'static str,
    },
    /// The query returned a negative count.
    #[error("the check's query returned {value} for {column}")]
    NegativeCount {
        /// The count's column.
        column: &'static str,
        /// The value.
        value: i64,
        /// The failed conversion.
        #[source]
        source: TryFromIntError,
    },
}

/// What running one check's query came to.
#[derive(Debug)]
pub enum Outcome {
    /// The query returned its two counts.
    Counted(Counts),
    /// The table or column a `cdmTable` or `cdmField` probe reads does not
    /// exist, with the database's error.
    Absent(CheckError),
    /// The query produced no counts.
    Failed(CheckError),
}

/// One check's result, with the Dashboard's result fields.
#[derive(Debug)]
#[non_exhaustive]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the four flags are the Dashboard's own result columns, kept one to one"
)]
pub struct CheckResult {
    /// The counts, absent when the query produced none (`numViolatedRows`,
    /// `pctViolatedRows` through [`Counts::fraction`], `numDenominatorRows`).
    pub counts: Option<Counts>,
    /// How long the query took (`executionTime`), absent when it was not run.
    pub execution_time: Option<Duration>,
    /// The query (`queryText`).
    pub query_text: String,
    /// The check (`checkName`).
    pub check_name: CheckName,
    /// The level (`checkLevel`).
    pub check_level: CheckLevel,
    /// The catalogue's description, placeholders filled in
    /// (`checkDescription`).
    pub check_description: String,
    /// The table, upper case (`cdmTableName`).
    pub cdm_table_name: String,
    /// The field, upper case, for a field check (`cdmFieldName`).
    pub cdm_field_name: Option<String>,
    /// The Dashboard's SQL template (`sqlFile`).
    pub sql_file: &'static str,
    /// The Kahn category (`category`).
    pub category: KahnCategory,
    /// The Kahn subcategory (`subcategory`).
    pub subcategory: Option<KahnSubcategory>,
    /// The Kahn context (`context`).
    pub context: KahnContext,
    /// Why the query produced no counts (`error`).
    pub error: Option<CheckError>,
    /// The check id (`checkId`).
    pub check_id: String,
    /// Whether the check failed (`failed`).
    pub failed: bool,
    /// Whether the check passed (`passed`): neither failed nor in error.
    pub passed: bool,
    /// Whether the check could not be evaluated (`isError`).
    pub is_error: bool,
    /// Whether the check does not apply to the data (`notApplicable`).
    pub not_applicable: bool,
    /// Why the check does not apply (`notApplicableReason`).
    pub not_applicable_reason: Option<String>,
    /// The threshold the check was evaluated against (`thresholdValue`).
    pub threshold_value: Option<Percentage>,
    /// The Dashboard's notes for the check (`notesValue`).
    pub notes_value: Option<&'static str>,
}

/// Evaluates `check` against the `outcome` of its query.
///
/// Counts are judged by [`Counts::exceed`]. A missing table or column fails a
/// `cdmTable` or `cdmField` probe and is not an error (DQD v2.9.0
/// `R/evaluateThresholds.R` lines 156 to 162); for any other check it is an
/// error. An error result is neither failed nor passed (lines 163 to 166).
#[must_use]
pub fn evaluate(check: &Check, outcome: Outcome, execution_time: Option<Duration>) -> CheckResult {
    let name = check.name();
    let threshold = check.threshold();
    let (counts, error, failed, is_error) = match outcome {
        Outcome::Counted(counts) => (Some(counts), None, counts.exceed(threshold), false),
        Outcome::Absent(error) if name.is_probe() => (None, Some(error), true, false),
        Outcome::Absent(error) | Outcome::Failed(error) => (None, Some(error), false, true),
    };
    // TODO(#405): the not-applicable status of DQD v2.9.0
    // `R/calculateNotApplicableStatus.R`, which needs the completeness checks.
    CheckResult {
        counts,
        execution_time,
        query_text: check.sql(),
        check_name: name,
        check_level: name.level(),
        check_description: check.description(),
        cdm_table_name: check.cdm_table_name(),
        cdm_field_name: check.cdm_field_name(),
        sql_file: name.sql_file(),
        category: name.category(),
        subcategory: name.subcategory(),
        context: name.context(),
        error,
        check_id: check.id(),
        failed,
        passed: !failed && !is_error,
        is_error,
        not_applicable: false,
        not_applicable_reason: None,
        threshold_value: threshold,
        notes_value: check.notes(),
    }
}

/// The counts of one Kahn category in a run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CategoryCounts {
    /// The checks of the category.
    pub total: usize,
    /// The checks of the category that failed.
    pub failed: usize,
    /// The checks of the category that passed.
    pub passed: usize,
}

/// The overview of a run, after DQD v2.9.0 `R/summarizeResults.R`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    /// Every result (`countTotal`).
    pub count_total: usize,
    /// The results whose `passed` is true (`countPassed`).
    pub count_passed: usize,
    /// The results that carry an error (`countErrorFailed`).
    pub count_error_failed: usize,
    /// The failed results that carry no error (`countThresholdFailed`).
    pub count_threshold_failed: usize,
    /// The failed results (`countOverallFailed`).
    pub count_overall_failed: usize,
    /// The Conformance results.
    pub conformance: CategoryCounts,
    /// The Completeness results.
    pub completeness: CategoryCounts,
    /// The Plausibility results.
    pub plausibility: CategoryCounts,
}

impl Summary {
    /// Counts `results`.
    ///
    /// Every count is the Dashboard's except `count_passed`, which counts the
    /// rows whose `passed` is true; the Dashboard subtracts the failed rows
    /// from the total, which counts an error as a pass.
    #[must_use]
    pub fn new(results: &[CheckResult]) -> Self {
        let mut summary = Self::default();
        for result in results {
            summary.count_total += 1;
            summary.count_passed += usize::from(result.passed);
            summary.count_error_failed += usize::from(result.error.is_some());
            summary.count_threshold_failed += usize::from(result.failed && result.error.is_none());
            summary.count_overall_failed += usize::from(result.failed);
            let category = match result.category {
                KahnCategory::Conformance => &mut summary.conformance,
                KahnCategory::Completeness => &mut summary.completeness,
                KahnCategory::Plausibility => &mut summary.plausibility,
            };
            category.total += 1;
            category.failed += usize::from(result.failed);
            category.passed += usize::from(result.passed);
        }
        summary
    }

    /// Returns the percentage of passes among passes and failures
    /// (`percentPassed`), unrounded, or `None` when there are neither.
    #[must_use]
    pub fn percent_passed(&self) -> Option<f64> {
        percent(
            self.count_passed,
            self.count_passed + self.count_overall_failed,
        )
    }

    /// Returns the percentage of failures among passes and failures
    /// (`percentFailed`), unrounded, or `None` when there are neither.
    #[must_use]
    pub fn percent_failed(&self) -> Option<f64> {
        percent(
            self.count_overall_failed,
            self.count_passed + self.count_overall_failed,
        )
    }
}

/// Returns `part` as a percentage of `whole`, or `None` when `whole` is 0.
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "a run counts far fewer checks than f64 represents exactly"
)]
fn percent(part: usize, whole: usize) -> Option<f64> {
    (whole != 0).then(|| part as f64 * 100.0 / whole as f64)
}
