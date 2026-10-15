// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A real profile: HL7 Europe Base `patient-eu-core` 2.0.1, read with its
//! dependency closure from the packages `scripts/vendor/profile-packages.sh`
//! fetches into the ignored `docs/specs/fhir-packages/`. Each test returns
//! early when the trees are absent, as the other build-time corpora do.

use std::path::Path;
use std::path::PathBuf;

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::set::PackageSet;
use serde_json::json;

use crate::support::TestResult;
use crate::support::core;
use crate::support::locations;
use crate::support::validate;

/// The profile under test.
const PATIENT_EU_CORE: &str = "http://hl7.eu/fhir/base/StructureDefinition/patient-eu-core|2.0.1";

/// The directory the profile packages are fetched into.
fn fetched() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/specs/fhir-packages")
}

/// The package set of every fetched profile package and R4 core, `None`
/// when the packages are not fetched.
fn eu_set() -> TestResult<Option<PackageSet>> {
    let root = fetched();
    if !root
        .join("hl7.fhir.eu.base#2.0.1/package/package.json")
        .is_file()
    {
        return Ok(None);
    }
    let mut paths = vec![core()];
    for entry in std::fs::read_dir(&root)? {
        let path = entry?.path();
        if path.join("package/package.json").is_file() {
            paths.push(path);
        }
    }
    Ok(Some(PackageSet::load(paths)?))
}

/// A synthetic Patient for `patient-eu-core`.
fn patient() -> serde_json::Value {
    json!({
        "resourceType": "Patient",
        "meta": {"profile": ["http://hl7.eu/fhir/base/StructureDefinition/patient-eu-core"]},
        "name": [{"family": "Doe", "given": ["Ada"]}],
        "gender": "female",
        "birthDate": "1970-01-01"
    })
}

#[test]
fn the_eu_closure_loads_complete() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    assert!(
        set.packages().len() >= 18,
        "R4 core and the 17 profile packages"
    );
    Ok(())
}

#[test]
fn a_conforming_patient_raises_no_error_against_patient_eu_core() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    let report = validate(&set, &patient(), Some(PATIENT_EU_CORE))?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    assert!(
        !report.unevaluated().is_empty(),
        "the profile's slicing and invariants are named"
    );
    Ok(())
}

#[test]
fn a_patient_without_the_birth_date_patient_eu_core_requires_is_refused() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    let mut resource = patient();
    resource
        .as_object_mut()
        .expect("an object")
        .remove("birthDate");
    let report = validate(&set, &resource, Some(PATIENT_EU_CORE))?;
    assert_eq!(locations(&report, MessageId::MinCardinality), ["Patient"]);
    Ok(())
}
