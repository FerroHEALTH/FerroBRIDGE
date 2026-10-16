// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The R4 core invariants over the HL7 R4 examples package 4.0.1: no
//! invariant fails on an example but the recorded ones, each with the
//! evidence from the example.
//!
//! The suite reads a curated subset, every `AllergyIntolerance`,
//! `Composition`, `Condition`, `Observation` and `Patient` example and every
//! `Bundle` example under 64 KiB, with the four examples whose extension
//! urls carry a fragment, because the whole package of 5309 resources
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

/// The examples of other types the suite reads too: each carries an
/// extension whose `url` has a `#` fragment, an unknown extension that never
/// ends the validation.
const FRAGMENT_URLS: [&str; 4] = [
    "Basic-referral.json",
    "CarePlan-preg.json",
    "Media-1.2.840.11361907579238403408700.3.1.04.19970327150033.json",
    "Patient-dicom.json",
];

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
        let typed =
            TYPES.contains(&kind) && (kind != "Bundle" || path.metadata()?.len() < BUNDLE_SIZE);
        if !typed && !FRAGMENT_URLS.contains(&name.as_str()) {
            continue;
        }
        read += 1;
        let resource: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        let report = validator.validate(&resource)?;
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
    Ok(())
}
