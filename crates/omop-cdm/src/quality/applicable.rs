// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Dashboard's not-applicable status: a check of a table that does not
//! exist or holds no row, or of a field that does not exist or holds no
//! value, is neither failed nor passed.
//!
//! [`apply`] reads the facts from the run's own results, `cdmTable` and
//! `cdmField` for what is missing and `measureValueCompleteness` for what is
//! empty, as DQD v2.9.0 `R/calculateNotApplicableStatus.R` does, and only
//! when the run holds all three checks (`R/evaluateThresholds.R` lines 177 to
//! 181). The concept-level facts of that file belong to checks the port does
//! not run, so they never hold here.

use std::collections::BTreeSet;

use crate::quality::check::CheckName;
use crate::quality::evaluate::CheckResult;

/// The facts the rules read, gathered from one run's results.
struct Facts<'r> {
    missing_tables: BTreeSet<&'r str>,
    missing_fields: BTreeSet<(&'r str, &'r str)>,
    empty_tables: BTreeSet<&'r str>,
    empty_fields: BTreeSet<(&'r str, &'r str)>,
}

impl<'r> Facts<'r> {
    /// Gathers the facts from `results`.
    fn of(results: &'r [CheckResult]) -> Self {
        let mut facts = Self {
            missing_tables: BTreeSet::new(),
            missing_fields: BTreeSet::new(),
            empty_tables: BTreeSet::new(),
            empty_fields: BTreeSet::new(),
        };
        for result in results {
            let table = result.cdm_table_name.as_str();
            let field = result.cdm_field_name.as_deref();
            match (result.check_name, field) {
                (CheckName::CdmTable, _) if result.failed => {
                    facts.missing_tables.insert(table);
                }
                (CheckName::CdmField, Some(field)) if result.failed => {
                    facts.missing_fields.insert((table, field));
                }
                (CheckName::MeasureValueCompleteness, Some(field)) => {
                    if let Some(counts) = result.counts {
                        if counts.denominator == 0 {
                            facts.empty_tables.insert(table);
                        }
                        if counts.denominator == counts.violated {
                            facts.empty_fields.insert((table, field));
                        }
                    }
                }
                _ => {}
            }
        }
        facts
    }

    /// Returns why `result` does not apply, or `None` when it does
    /// (`R/calculateNotApplicableStatus.R` lines 52 to 100 and 212 to 244).
    fn reason(&self, result: &CheckResult) -> Option<String> {
        let table = result.cdm_table_name.as_str();
        let field = result.cdm_field_name.as_deref();
        let table_missing = self.missing_tables.contains(table);
        let field_missing =
            field.is_some_and(|field| self.missing_fields.contains(&(table, field)));
        let table_empty = self.empty_tables.contains(table);
        let field_empty = field.is_some_and(|field| self.empty_fields.contains(&(table, field)));
        let applies = match result.check_name {
            CheckName::MeasureConditionEraCompleteness => {
                let occurrence = "CONDITION_OCCURRENCE";
                return (self.missing_tables.contains(occurrence)
                    || self.empty_tables.contains(occurrence))
                .then(|| String::from("Table CONDITION_OCCURRENCE is empty."));
            }
            CheckName::MeasurePersonCompleteness | CheckName::CdmField => !table_missing,
            CheckName::CdmTable => true,
            _ if table_missing || field_missing => false,
            _ if result.is_error => true,
            _ if table_empty => false,
            CheckName::MeasureValueCompleteness => true,
            _ => !field_empty,
        };
        if applies {
            return None;
        }
        let field = field.unwrap_or_default();
        Some(if table_missing {
            format!("Table {table} does not exist.")
        } else if field_missing {
            format!("Field {table}.{field} does not exist.")
        } else if table_empty {
            format!("Table {table} is empty.")
        } else {
            format!("Field {table}.{field} is not populated.")
        })
    }
}

/// Marks every result of `results` that does not apply, clearing its
/// `failed` and `passed`, when the run holds results of `cdmTable`,
/// `cdmField` and `measureValueCompleteness`.
pub fn apply(results: &mut [CheckResult]) {
    let present: BTreeSet<CheckName> = results.iter().map(|result| result.check_name).collect();
    let all = [
        CheckName::CdmTable,
        CheckName::CdmField,
        CheckName::MeasureValueCompleteness,
    ];
    if !all.iter().all(|name| present.contains(name)) {
        return;
    }
    let reasons: Vec<Option<String>> = {
        let facts = Facts::of(results);
        results.iter().map(|result| facts.reason(result)).collect()
    };
    for (result, reason) in results.iter_mut().zip(reasons) {
        result.not_applicable = reason.is_some();
        if result.not_applicable {
            result.failed = false;
        }
        result.passed = !result.failed && !result.is_error && !result.not_applicable;
        result.not_applicable_reason = reason;
    }
}
