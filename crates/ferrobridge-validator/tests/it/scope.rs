// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A version-less canonical a definition names resolves within the
//! dependency closure of the package that carries the definition (the
//! `dependencies` of the package manifest, <https://hl7.org/fhir/packages.html>),
//! and in the whole set only when the closure does not carry it.

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::set::PackageSet;
use serde_json::json;

use crate::support::BASE;
use crate::support::Synthetic;
use crate::support::TestResult;
use crate::support::core;
use crate::support::extension;
use crate::support::locations;
use crate::support::package;
use crate::support::profile;
use crate::support::validate;

/// The base package `example.base` at `version`, carrying the extension
/// `flag` whose value is of `value_type`.
fn base(version: &str, value_type: &str) -> TestResult<Synthetic> {
    let mut flag = extension("flag", value_type, false);
    flag["version"] = json!(version);
    package(
        "example.base",
        version,
        &[("hl7.fhir.r4.core", "4.0.1")],
        &[flag],
    )
}

/// A Patient profile slicing `extension` by url, with a slice taking its
/// definition from the version-less canonical of `flag`.
fn flagged_patient() -> serde_json::Value {
    profile(
        "scoped-patient",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.extension", "path": "Patient.extension", "min": 0, "max": "*",
             "slicing": {"discriminator": [{"type": "value", "path": "url"}], "rules": "open"}},
            {"id": "Patient.extension:flag", "path": "Patient.extension", "sliceName": "flag",
             "min": 0, "max": "1", "type": [{"code": "Extension", "profile": [format!("{BASE}flag")]}]}
        ]),
    )
}

/// The package set of both base versions and a profile package naming
/// `dependencies`.
fn set_depending_on(dependencies: &[(&str, &str)]) -> TestResult<(Vec<Synthetic>, PackageSet)> {
    let old = base("2.0.0", "boolean")?;
    let new = base("2.0.1", "string")?;
    let profiles = package(
        "example.profiles",
        "0.1.0",
        dependencies,
        &[flagged_patient()],
    )?;
    let set = PackageSet::load([
        core(),
        old.path().to_owned(),
        new.path().to_owned(),
        profiles.path().to_owned(),
    ])?;
    Ok((vec![old, new, profiles], set))
}

/// A Patient carrying the extension `flag` with a boolean value.
fn flagged() -> serde_json::Value {
    json!({"resourceType": "Patient",
           "extension": [{"url": format!("{BASE}flag"), "valueBoolean": true}]})
}

#[test]
fn a_bare_canonical_resolves_to_the_version_the_package_depends_on() -> TestResult {
    let (_packages, set) =
        set_depending_on(&[("hl7.fhir.r4.core", "4.0.1"), ("example.base", "2.0.0")])?;
    let report = validate(&set, &flagged(), Some(&format!("{BASE}scoped-patient")))?;
    assert!(
        report.is_valid(),
        "2.0.0 defines a boolean flag: {:#?}",
        report.issues()
    );
    Ok(())
}

#[test]
fn the_same_canonical_resolves_to_the_other_version_for_its_dependent() -> TestResult {
    let (_packages, set) =
        set_depending_on(&[("hl7.fhir.r4.core", "4.0.1"), ("example.base", "2.0.1")])?;
    let report = validate(&set, &flagged(), Some(&format!("{BASE}scoped-patient")))?;
    assert_eq!(
        locations(&report, MessageId::ChoiceTypeNotAllowed),
        ["Patient.extension[0].value.ofType(boolean)"]
    );
    Ok(())
}

#[test]
fn a_canonical_outside_the_closure_resolves_in_the_whole_set() -> TestResult {
    let (_packages, set) = set_depending_on(&[("hl7.fhir.r4.core", "4.0.1")])?;
    let report = validate(&set, &flagged(), Some(&format!("{BASE}scoped-patient")))?;
    assert_eq!(
        locations(&report, MessageId::ChoiceTypeNotAllowed),
        ["Patient.extension[0].value.ofType(boolean)"],
        "the latest version, 2.0.1, defines a string flag"
    );
    Ok(())
}
