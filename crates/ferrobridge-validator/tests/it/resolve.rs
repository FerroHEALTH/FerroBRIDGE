// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Canonical resolution (<https://hl7.org/fhir/R4/references.html#canonical>,
//! §2.3.0.5 and §2.3.0.6) and the snapshot a profile SHALL carry.

use ferrobridge_validator::set::PackageSet;
use ferrobridge_validator::set::ResolveError;
use ferrobridge_validator::validate::ValidateError;
use serde_json::json;

use crate::support::BASE;
use crate::support::TestResult;
use crate::support::core;
use crate::support::extension;
use crate::support::package;
use crate::support::profile;
use crate::support::set_with;
use crate::support::validate;

/// Two versions of one extension, in two packages.
fn two_versions() -> TestResult<(
    crate::support::Synthetic,
    crate::support::Synthetic,
    PackageSet,
)> {
    let mut old = extension("flag", "boolean", false);
    old["version"] = json!("1.0.0");
    let mut new = extension("flag", "string", false);
    new["version"] = json!("1.2.0");
    let first = package("example.a", "1.0.0", &[], &[old])?;
    let second = package("example.a", "1.2.0", &[], &[new])?;
    let set = PackageSet::load([first.path(), second.path()])?;
    Ok((first, second, set))
}

/// The value type the snapshot of `definition` allows.
fn value_type(
    definition: &fhir_types::r4::structure_definition::StructureDefinition,
) -> Option<String> {
    definition
        .snapshot
        .as_ref()?
        .element
        .iter()
        .find(|element| element.path.value.as_deref() == Some("Extension.value[x]"))?
        .r#type
        .first()?
        .code
        .value
        .clone()
}

#[test]
fn a_versioned_canonical_resolves_that_version() -> TestResult {
    let (_first, _second, set) = two_versions()?;
    let definition = set.structure_definition(&format!("{BASE}flag|1.0.0"))?;
    assert_eq!(value_type(&definition).as_deref(), Some("boolean"));
    Ok(())
}

#[test]
fn a_bare_canonical_resolves_the_latest_version() -> TestResult {
    let (_first, _second, set) = two_versions()?;
    let definition = set.structure_definition(&format!("{BASE}flag"))?;
    assert_eq!(value_type(&definition).as_deref(), Some("string"));
    Ok(())
}

#[test]
fn a_version_the_set_lacks_is_not_found() -> TestResult {
    let (_first, _second, set) = two_versions()?;
    let error = set
        .structure_definition(&format!("{BASE}flag|9.9.9"))
        .expect_err("no 9.9.9");
    assert!(
        matches!(error, ResolveError::NotFound { .. }),
        "got {error}"
    );
    Ok(())
}

#[test]
fn a_canonical_with_a_fragment_is_refused() -> TestResult {
    let (_first, _second, set) = two_versions()?;
    let error = set
        .structure_definition(&format!("{BASE}flag|1.0.0#inner"))
        .expect_err("a fragment");
    assert!(
        matches!(error, ResolveError::Fragment { .. }),
        "got {error}"
    );
    Ok(())
}

#[test]
fn two_different_definitions_under_one_version_are_ambiguous() -> TestResult {
    let mut one = extension("flag", "boolean", false);
    one["version"] = json!("1.0.0");
    let mut other = extension("flag", "string", false);
    other["version"] = json!("1.0.0");
    let first = package("example.a", "1.0.0", &[], &[one])?;
    let second = package("example.b", "1.0.0", &[], &[other])?;
    let set = PackageSet::load([first.path(), second.path()])?;
    let error = set
        .structure_definition(&format!("{BASE}flag"))
        .expect_err("two definitions");
    assert!(
        matches!(error, ResolveError::Ambiguous { .. }),
        "got {error}"
    );
    Ok(())
}

#[test]
fn a_core_definition_resolves_from_the_vendored_package() -> TestResult {
    let set = PackageSet::load([core()])?;
    let patient =
        set.structure_definition("http://hl7.org/fhir/StructureDefinition/Patient|4.0.1")?;
    assert_eq!(patient.r#type.value.as_deref(), Some("Patient"));
    Ok(())
}

#[test]
fn a_profile_without_a_snapshot_is_refused_by_its_url() -> TestResult {
    let mut definition = profile("no-snapshot", "Patient", &json!([]));
    definition
        .as_object_mut()
        .expect("an object")
        .remove("snapshot");
    definition["differential"] = json!({"element": [{"id": "Patient", "path": "Patient"}]});
    let (_synthetic, set) = set_with(&[definition])?;
    let error = validate(
        &set,
        &json!({"resourceType": "Patient"}),
        Some(&format!("{BASE}no-snapshot")),
    )
    .expect_err("no snapshot");
    let error = error.downcast::<ValidateError>()?;
    match *error {
        ValidateError::NoSnapshot { url } => assert_eq!(url, format!("{BASE}no-snapshot")),
        other => panic!("expected NoSnapshot, got {other}"),
    }
    Ok(())
}

#[test]
fn an_unknown_profile_is_an_error_naming_it() -> TestResult {
    let set = PackageSet::load([core()])?;
    let error = validate(
        &set,
        &json!({"resourceType": "Patient"}),
        Some(&format!("{BASE}absent")),
    )
    .expect_err("absent profile");
    let error = error.downcast::<ValidateError>()?;
    assert!(
        matches!(*error, ValidateError::Resolve(ResolveError::NotFound { ref canonical }) if canonical.ends_with("absent")),
        "got {error}"
    );
    Ok(())
}
