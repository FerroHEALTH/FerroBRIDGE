// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A dependency of another FHIR release: held, never read, and refused when
//! a profile or an instance reaches into it. The set is read in the release
//! its core dependency defines (FHIR 6.0.0-ballot4 packages.html
//! §2.1.10.3); the rest is our own design, where that text is silent.

use std::fs;

use ferrobridge_validator::package::Package;
use ferrobridge_validator::release::R4;
use ferrobridge_validator::set::LoadError;
use ferrobridge_validator::set::PackageSet;
use ferrobridge_validator::set::ResolveError;
use ferrobridge_validator::validate::ValidateError;
use ferrobridge_validator::validate::Validator;
use serde_json::json;

use crate::support::BASE;
use crate::support::Synthetic;
use crate::support::TestResult;
use crate::support::core;
use crate::support::extension;
use crate::support::instance;
use crate::support::package;
use crate::support::patient;
use crate::support::profile;

/// Sets the member `key` of the manifest of `synthetic` to `value`.
fn declare(synthetic: &Synthetic, key: &str, value: serde_json::Value) -> TestResult {
    let path = synthetic.path().join("package/package.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    manifest[key] = value;
    fs::write(&path, serde_json::to_vec_pretty(&manifest)?)?;
    Ok(())
}

/// An R4 guide depending on R4 core and on `example.models` 1.0.0, a
/// package of FHIR 5.0.0 that depends on an R5 terminology package the set
/// does not hold, and defines a profile and an extension.
fn guide_and_models() -> TestResult<(Synthetic, Synthetic)> {
    let guide = package(
        "example.guide",
        "1.0.0",
        &[("hl7.fhir.r4.core", "4.0.1"), ("example.models", "1.0.0")],
        &[extension("flag", "boolean", false)],
    )?;
    declare(&guide, "fhirVersions", json!(["4.0.1"]))?;
    let models = package(
        "example.models",
        "1.0.0",
        &[("example.terminology.r5", "7.1.0")],
        &[
            profile("model", "Patient", &json!([])),
            extension("r5-flag", "boolean", false),
        ],
    )?;
    declare(&models, "fhirVersions", json!(["5.0.0"]))?;
    Ok((guide, models))
}

#[test]
fn a_package_reads_its_fhir_versions_under_either_name() -> TestResult {
    let current = package("example.a", "1.0.0", &[], &[])?;
    declare(&current, "fhirVersions", json!(["5.0.0"]))?;
    assert_eq!(Package::read(current.path())?.fhir_versions(), ["5.0.0"]);
    let older = package("example.b", "1.0.0", &[], &[])?;
    declare(&older, "fhir-version-list", json!(["3.0.2"]))?;
    assert_eq!(Package::read(older.path())?.fhir_versions(), ["3.0.2"]);
    let silent = package("example.c", "1.0.0", &[], &[])?;
    assert!(Package::read(silent.path())?.fhir_versions().is_empty());
    Ok(())
}

#[test]
fn a_dependency_of_another_release_is_held_and_not_read() -> TestResult {
    let (guide, models) = guide_and_models()?;
    let set = PackageSet::<R4>::load([core(), guide.path().to_owned(), models.path().to_owned()])?;
    let read: Vec<&str> = set.packages().iter().map(Package::name).collect();
    assert_eq!(read, ["example.guide", "hl7.fhir.r4.core"]);
    let held: Vec<&str> = set.other_releases().iter().map(Package::name).collect();
    assert_eq!(held, ["example.models"]);
    set.structure_definition(&format!("{BASE}flag"))?;
    Ok(())
}

#[test]
fn a_dependency_of_the_set_release_still_needs_its_own_dependencies() -> TestResult {
    let (guide, models) = guide_and_models()?;
    declare(&models, "fhirVersions", json!(["4.0.1", "5.0.0"]))?;
    let error = PackageSet::<R4>::load([core(), guide.path().to_owned(), models.path().to_owned()])
        .expect_err("example.terminology.r5 is absent");
    match error {
        LoadError::MissingDependency {
            package,
            dependency,
            ..
        } => {
            assert_eq!(package, "example.models");
            assert_eq!(dependency, "example.terminology.r5");
        }
        other => panic!("expected a missing dependency, got {other}"),
    }
    Ok(())
}

#[test]
fn a_core_dependency_of_another_release_names_the_release_without_fhir_versions() -> TestResult {
    let guide = package(
        "example.guide",
        "1.0.0",
        &[("hl7.fhir.r4.core", "4.0.1"), ("example.models", "1.0.0")],
        &[],
    )?;
    let models = package(
        "example.models",
        "1.0.0",
        &[("hl7.fhir.r5.core", "5.0.0")],
        &[],
    )?;
    let set = PackageSet::<R4>::load([core(), guide.path().to_owned(), models.path().to_owned()])?;
    let held: Vec<&str> = set.other_releases().iter().map(Package::name).collect();
    assert_eq!(held, ["example.models"]);
    Ok(())
}

#[test]
fn a_dependency_of_another_release_must_still_be_given() -> TestResult {
    let (guide, _models) = guide_and_models()?;
    let error = PackageSet::<R4>::load([core(), guide.path().to_owned()])
        .expect_err("example.models is absent");
    assert!(
        matches!(&error, LoadError::MissingDependency { dependency, .. } if dependency == "example.models"),
        "got {error}"
    );
    Ok(())
}

#[test]
fn a_canonical_only_another_release_defines_is_refused_by_package() -> TestResult {
    let (guide, models) = guide_and_models()?;
    let set = PackageSet::<R4>::load([core(), guide.path().to_owned(), models.path().to_owned()])?;
    let error = set
        .structure_definition(&format!("{BASE}model|0.1.0"))
        .expect_err("the R5 package is not read");
    match error {
        ResolveError::OtherRelease {
            canonical,
            package,
            release,
        } => {
            assert_eq!(canonical, format!("{BASE}model|0.1.0"));
            assert_eq!(package, "example.models#1.0.0");
            assert_eq!(release, "R4");
        }
        other => panic!("expected another release, got {other}"),
    }
    assert!(
        matches!(
            set.structure_definition(&format!("{BASE}model|9.9.9")),
            Err(ResolveError::NotFound { .. })
        ),
        "a version the other release does not define stays not found"
    );
    assert!(
        matches!(
            set.value_set(&format!("{BASE}model")),
            Err(ResolveError::NotFound { .. })
        ),
        "a canonical of another type stays not found"
    );
    Ok(())
}

#[test]
fn a_profile_of_another_release_is_refused_with_a_typed_error() -> TestResult {
    let (guide, models) = guide_and_models()?;
    let set = PackageSet::<R4>::load([core(), guide.path().to_owned(), models.path().to_owned()])?;
    let error = Validator::new(&set)
        .validate_against(&instance(&patient())?, &format!("{BASE}model"))
        .expect_err("the profile is R5");
    assert!(
        matches!(
            &error,
            ValidateError::Resolve(ResolveError::OtherRelease { package, .. })
                if package == "example.models#1.0.0"
        ),
        "got {error}"
    );
    Ok(())
}

#[test]
fn an_extension_of_another_release_is_refused_with_a_typed_error() -> TestResult {
    let (guide, models) = guide_and_models()?;
    let set = PackageSet::<R4>::load([core(), guide.path().to_owned(), models.path().to_owned()])?;
    let mut resource = patient();
    resource["extension"] = json!([{"url": format!("{BASE}r5-flag"), "valueBoolean": true}]);
    let error = Validator::new(&set)
        .validate(&instance(&resource)?)
        .expect_err("the extension is defined in R5");
    assert!(
        matches!(
            &error,
            ValidateError::Resolve(ResolveError::OtherRelease { canonical, .. })
                if *canonical == format!("{BASE}r5-flag")
        ),
        "got {error}"
    );
    Ok(())
}
