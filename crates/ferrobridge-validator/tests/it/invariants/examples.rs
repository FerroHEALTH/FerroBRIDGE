// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The R4 core invariants over the HL7 R4 examples package 4.0.1: no
//! invariant fails on an example but the recorded ones, each with the
//! evidence from the example.
//!
//! The suite reads a curated subset, every `AllergyIntolerance`,
//! `Composition`, `Condition`, `Observation` and `Patient` example and every
//! `Bundle` example under 64 KiB, because the whole package of 5309 resources
//! takes about thirteen minutes in a debug build.

use std::collections::BTreeSet;

use ferrobridge_testkit::examples::Package;
use ferrobridge_validator::issue::Rule;
use ferrobridge_validator::issue::Severity;
use ferrobridge_validator::set::PackageSet;
use ferrobridge_validator::validate::Validator;
use fhir_types::codec::Value;

use crate::support::TestResult;
use crate::support::core;

/// The resource types whose examples the suite reads.
const TYPES: [&str; 6] = [
    "AllergyIntolerance",
    "Bundle",
    "Composition",
    "Condition",
    "Observation",
    "Patient",
];

/// The size under which a Bundle example is read.
const BUNDLE_SIZE: u64 = 64 * 1024;

/// The invariant failures the examples cause themselves: the file, the key,
/// the location and the evidence.
const RECORDED: [(&str, &str, &str, &str); 0] = [];

/// The examples the validator refuses to validate, with the error: an
/// extension `url` with a `#` fragment ends the validation with a resolution
/// error instead of an issue at the extension.
const REFUSED: [(&str, &str); 1] = [(
    "Patient-dicom.json",
    "http://nema.org/fhir/extensions#0010:1010 names a contained resource, which the package set does not resolve",
)];

/// Whether the R4 examples package is fetched; under CI (`CI=true`) an
/// absent package is an error.
fn fetched() -> TestResult<bool> {
    if Package::R4.fetched() {
        return Ok(true);
    }
    if std::env::var("CI").is_ok_and(|value| value == "true") {
        return Err(
            "the R4 examples package is absent under CI; run scripts/vendor/fhir-packages.sh --build-time"
                .into(),
        );
    }
    Ok(false)
}

#[test]
fn no_core_invariant_fails_on_an_r4_example_but_the_recorded_ones() -> TestResult {
    if !fetched()? {
        return Ok(());
    }
    let set = PackageSet::load([core()])?;
    let validator = Validator::new(&set);
    let mut found = BTreeSet::new();
    let mut refused = BTreeSet::new();
    let mut read = 0_usize;
    for path in Package::R4.resources()? {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        let Some(kind) = name.split('-').next() else {
            continue;
        };
        if !TYPES.contains(&kind) || (kind == "Bundle" && path.metadata()?.len() >= BUNDLE_SIZE) {
            continue;
        }
        read += 1;
        let resource: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        let report = match validator.validate(&resource) {
            Ok(report) => report,
            Err(error) => {
                refused.insert((name, error.to_string()));
                continue;
            }
        };
        for issue in report.issues() {
            if let Rule::Constraint { key, .. } = &issue.rule
                && issue.severity == Severity::Error
            {
                found.insert((name.clone(), key.clone(), issue.expression.clone()));
            }
        }
    }
    assert!(read > 100, "only {read} examples were read");
    let recorded: BTreeSet<(String, String, String)> = RECORDED
        .iter()
        .map(|(file, key, location, _)| {
            (
                (*file).to_owned(),
                (*key).to_owned(),
                (*location).to_owned(),
            )
        })
        .collect();
    assert_eq!(found, recorded);
    let expected_refusals: BTreeSet<(String, String)> = REFUSED
        .iter()
        .map(|(file, error)| ((*file).to_owned(), (*error).to_owned()))
        .collect();
    assert_eq!(refused, expected_refusals);
    Ok(())
}
