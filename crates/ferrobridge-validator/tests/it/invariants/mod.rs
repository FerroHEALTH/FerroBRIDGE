// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Invariants (<https://hl7.org/fhir/R4/conformance-rules.html#constraints>,
//! §2.1.0.6): every constraint on every element in play, evaluated with
//! FHIRPath on each occurrence, or listed as unevaluated with the reason.

mod bundles;
mod core;
mod eu;
mod examples;
mod profiles;
mod seams;

use ferrobridge_validator::issue::Issue;
use ferrobridge_validator::issue::Rule;
use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Report;
use ferrobridge_validator::report::Unevaluated;
use ferrobridge_validator::set::PackageSet;

use crate::support::TestResult;
use crate::support::core;
use crate::support::validate;

/// The issues of `report` that report the invariant `key`.
pub(crate) fn failures<'r>(report: &'r Report, key: &str) -> Vec<&'r Issue> {
    report
        .issues()
        .iter()
        .filter(|issue| matches!(&issue.rule, Rule::Constraint { key: found, .. } if found == key))
        .collect()
}

/// The locations of the issues of `report` that report the invariant `key`.
pub(crate) fn failed_at(report: &Report, key: &str) -> Vec<String> {
    failures(report, key)
        .into_iter()
        .map(|issue| issue.expression.clone())
        .collect()
}

/// The listing entries of `report` for the invariant `key`.
pub(crate) fn listed<'r>(report: &'r Report, key: &str) -> Vec<&'r Unevaluated> {
    report
        .unevaluated()
        .iter()
        .filter(|entry| matches!(&entry.constraint, Constraint::Invariant { key: found, .. } if found == key))
        .collect()
}

/// Validates `resource` against the core definition of its type.
pub(crate) fn against_core(resource: &serde_json::Value) -> TestResult<Report> {
    let set = PackageSet::load([core()])?;
    validate(&set, resource, None)
}
