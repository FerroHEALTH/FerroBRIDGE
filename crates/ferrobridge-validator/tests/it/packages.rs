// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading packages from directories and `.tgz` files, and the dependency
//! closure (<https://hl7.org/fhir/packages.html>, §2.1.10).

use std::fs;

use ferrobridge_validator::package::Package;
use ferrobridge_validator::set::LoadError;
use ferrobridge_validator::set::PackageSet;
use serde_json::json;

use crate::support::BASE;
use crate::support::TestResult;
use crate::support::core;
use crate::support::extension;
use crate::support::package;
use crate::support::tarball;

#[test]
fn a_directory_package_reads_its_manifest_and_definitions() -> TestResult {
    let synthetic = package(
        "example.a",
        "1.0.0",
        &[],
        &[extension("flag", "boolean", false)],
    )?;
    let read = Package::read(synthetic.path())?;
    assert_eq!(read.name(), "example.a");
    assert_eq!(read.version(), "1.0.0");
    assert_eq!(read.structure_definitions(), 1);
    Ok(())
}

#[test]
fn a_tgz_package_reads_the_same_as_its_directory() -> TestResult {
    let synthetic = package(
        "example.a",
        "1.0.0",
        &[("hl7.fhir.r4.core", "4.0.1")],
        &[extension("flag", "boolean", false)],
    )?;
    let out = tempfile::tempdir()?;
    let archive = out.path().join("example.a-1.0.0.tgz");
    tarball(synthetic.path(), &archive)?;
    let read = Package::read(&archive)?;
    assert_eq!(read.name(), "example.a");
    assert_eq!(
        read.dependencies()
            .get("hl7.fhir.r4.core")
            .map(String::as_str),
        Some("4.0.1")
    );
    let set = PackageSet::load([core(), archive])?;
    let definition = set.structure_definition(&format!("{BASE}flag"))?;
    assert_eq!(definition.r#type.value.as_deref(), Some("Extension"));
    Ok(())
}

#[test]
fn the_vendored_core_package_reads_through_its_index() -> TestResult {
    let read = Package::read(&core())?;
    assert_eq!((read.name(), read.version()), ("hl7.fhir.r4.core", "4.0.1"));
    assert!(
        read.structure_definitions() > 600,
        "R4 core carries over 600 StructureDefinitions"
    );
    Ok(())
}

#[test]
fn a_missing_dependency_is_refused_by_name() -> TestResult {
    let synthetic = package("example.a", "1.0.0", &[("example.base", "2.0.1")], &[])?;
    let error = PackageSet::load([synthetic.path()]).expect_err("example.base is absent");
    match error {
        LoadError::MissingDependency {
            package,
            dependency,
            dependency_version,
            ..
        } => {
            assert_eq!(package, "example.a");
            assert_eq!(dependency, "example.base");
            assert_eq!(dependency_version, "2.0.1");
        }
        other => panic!("expected a missing dependency, got {other}"),
    }
    Ok(())
}

#[test]
fn a_dependency_at_another_version_is_missing() -> TestResult {
    let base = package("example.base", "2.0.0", &[], &[])?;
    let dependent = package("example.a", "1.0.0", &[("example.base", "2.0.1")], &[])?;
    let error = PackageSet::load([base.path(), dependent.path()]).expect_err("2.0.1 is absent");
    assert!(
        matches!(error, LoadError::MissingDependency { .. }),
        "got {error}"
    );
    Ok(())
}

#[test]
fn a_patch_wildcard_dependency_accepts_any_patch() -> TestResult {
    let base = package("example.base", "2.0.3", &[], &[])?;
    let dependent = package("example.a", "1.0.0", &[("example.base", "2.0.x")], &[])?;
    let set = PackageSet::load([base.path(), dependent.path()])?;
    assert_eq!(set.packages().len(), 2);
    Ok(())
}

#[test]
fn the_same_package_twice_is_refused() -> TestResult {
    let first = package("example.a", "1.0.0", &[], &[])?;
    let second = package("example.a", "1.0.0", &[], &[])?;
    let error = PackageSet::load([first.path(), second.path()]).expect_err("given twice");
    assert!(
        matches!(error, LoadError::DuplicatePackage { .. }),
        "got {error}"
    );
    Ok(())
}

#[test]
fn a_byte_order_mark_before_the_json_is_stripped() -> TestResult {
    let synthetic = package("example.a", "1.0.0", &[], &[])?;
    let manifest = synthetic.path().join("package/package.json");
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend(fs::read(&manifest)?);
    fs::write(&manifest, bytes)?;
    let definition = synthetic
        .path()
        .join("package/StructureDefinition-flag.json");
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend(serde_json::to_vec(&extension("flag", "boolean", false))?);
    fs::write(&definition, bytes)?;
    let set = PackageSet::load([synthetic.path()])?;
    set.structure_definition(&format!("{BASE}flag"))?;
    Ok(())
}

#[test]
fn a_path_that_is_no_package_is_refused() -> TestResult {
    let empty = tempfile::tempdir()?;
    let error = Package::read(empty.path()).expect_err("no package/package.json");
    assert!(
        matches!(
            error,
            ferrobridge_validator::package::PackageError::NoManifest { .. }
        ),
        "got {error}"
    );
    Ok(())
}

#[test]
fn a_package_index_naming_an_absent_file_is_refused() -> TestResult {
    let synthetic = package("example.a", "1.0.0", &[], &[])?;
    let index = json!({"index-version": 1, "files": [
        {"filename": "StructureDefinition-gone.json", "resourceType": "StructureDefinition"}
    ]});
    fs::write(
        synthetic.path().join("package/.index.json"),
        serde_json::to_vec(&index)?,
    )?;
    let error = Package::read(synthetic.path()).expect_err("the indexed file is absent");
    assert!(
        matches!(
            error,
            ferrobridge_validator::package::PackageError::IndexedFileMissing { .. }
        ),
        "got {error}"
    );
    Ok(())
}
