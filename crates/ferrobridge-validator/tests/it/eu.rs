// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A real profile: HL7 Europe Base `patient-eu-core` 2.0.1, read with its
//! dependency closure from the packages `scripts/vendor/profile-packages.sh`
//! fetches into the ignored `docs/specs/fhir-packages/`. Each test returns
//! early when the trees are absent, and fails under CI
//! ([`crate::support::profile_packages`]).

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::set::PackageSet;
use serde_json::json;

use crate::support::TestResult;
use crate::support::core;
use crate::support::locations;
use crate::support::profile_packages;
use crate::support::validate;

/// The profile under test.
const PATIENT_EU_CORE: &str = "http://hl7.eu/fhir/base/StructureDefinition/patient-eu-core|2.0.1";

/// The package set of every fetched profile package and R4 core, `None`
/// when the packages are not fetched outside CI.
pub(crate) fn eu_set() -> TestResult<Option<PackageSet>> {
    let Some(root) = profile_packages()? else {
        return Ok(None);
    };
    let mut paths = vec![core()];
    for entry in std::fs::read_dir(&root)? {
        let path = entry?.path();
        if path.join("package/package.json").is_file() {
            paths.push(path);
        }
    }
    Ok(Some(PackageSet::load(paths)?))
}

/// The emptyReason of a patient summary section with nothing to hold, which
/// `ips-comp-1` of `composition-eu-eps` asks of a required section without
/// entries.
pub(crate) fn empty_reason() -> serde_json::Value {
    json!({"coding": [{"system": "http://terminology.hl7.org/CodeSystem/list-empty-reason", "code": "unavailable"}]})
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
        set.packages().len() >= 20,
        "R4 core and the 19 profile packages"
    );
    Ok(())
}

#[test]
fn the_laboratory_and_mpd_profiles_resolve_by_canonical_and_version() -> TestResult {
    let Some(set) = eu_set()? else { return Ok(()) };
    for (canonical, kind) in [
        (
            "http://hl7.eu/fhir/laboratory/StructureDefinition/Bundle-eu-lab|2.0.0",
            "Bundle",
        ),
        (
            "http://hl7.eu/fhir/laboratory/StructureDefinition/Composition-eu-lab|2.0.0",
            "Composition",
        ),
        (
            "http://hl7.eu/fhir/laboratory/StructureDefinition/DiagnosticReport-eu-lab|2.0.0",
            "DiagnosticReport",
        ),
        (
            "http://hl7.eu/fhir/laboratory/StructureDefinition/Observation-resultslab-eu-lab|2.0.0",
            "Observation",
        ),
        (
            "http://hl7.eu/fhir/laboratory/StructureDefinition/Specimen-eu-lab|2.0.0",
            "Specimen",
        ),
        (
            "http://hl7.eu/fhir/mpd/StructureDefinition/MedicationRequest-eu-mpd|1.0.0",
            "MedicationRequest",
        ),
        (
            "http://hl7.eu/fhir/mpd/StructureDefinition/MedicationDispense-eu-mpd|1.0.0",
            "MedicationDispense",
        ),
        (
            "http://hl7.eu/fhir/mpd/StructureDefinition/Medication-eu-mpd|1.0.0",
            "Medication",
        ),
        (
            "http://hl7.eu/fhir/mpd/StructureDefinition/Dosage-eu-mpd|1.0.0",
            "Dosage",
        ),
    ] {
        let definition = set.structure_definition(canonical)?;
        let found = serde_json::to_value(definition.as_ref())?;
        assert_eq!(Some(kind), found["type"].as_str(), "{canonical}");
        assert!(
            found["snapshot"]["element"]
                .as_array()
                .is_some_and(|elements| !elements.is_empty()),
            "{canonical} carries a snapshot"
        );
    }
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
