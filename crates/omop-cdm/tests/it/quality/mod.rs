// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The port of the Data Quality Dashboard's conformance checks: the generated
//! catalogue, the plan, the evaluation, and, with the `database` feature, the
//! checks run against PostgreSQL.

mod catalogue;
mod evaluate;
mod plan;
#[cfg(feature = "database")]
mod run;

use std::error::Error;

use omop_cdm::quality::check::{Check, CheckName};
use omop_cdm::quality::plan::{Plan, PlanOptions};

/// Returns the default plan's `name` check on `table`, or on `field` of
/// `table` when a field is given.
pub(crate) fn planned(
    name: CheckName,
    table: &str,
    field: Option<&str>,
) -> Result<Check, Box<dyn Error>> {
    find(&Plan::new(&PlanOptions::default())?, name, table, field)
        .ok_or_else(|| format!("the plan has no {} on {table}.{field:?}", name.as_str()).into())
}

/// Returns the `name` check of `plan` on `table` and `field`, if planned.
pub(crate) fn find(
    plan: &Plan,
    name: CheckName,
    table: &str,
    field: Option<&str>,
) -> Option<Check> {
    plan.checks()
        .iter()
        .find(|check| {
            check.name() == name
                && check.table().name == table
                && check.column().map(|column| column.name) == field
        })
        .cloned()
}
