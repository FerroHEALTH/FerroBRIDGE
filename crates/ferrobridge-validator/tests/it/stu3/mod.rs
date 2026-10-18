// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! STU3 profiles validating STU3 resources: the package set read as
//! `Stu3`, the instance walked over the STU3 element table, and the STU3
//! profile shapes (one `type.profile`, a `valueSetReference` binding,
//! `contextType` with context strings) read through the release-neutral
//! definition read (<https://hl7.org/fhir/STU3/profiling.html>).

mod bindings;
mod contexts;
mod invariants;
mod slicing;
mod structure;

use std::path::Path;
use std::path::PathBuf;

use ferrobridge_validator::issue::Severity;
use ferrobridge_validator::release::Stu3;
use ferrobridge_validator::report::Report;
use ferrobridge_validator::set::PackageSet;
use ferrobridge_validator::validate::Validator;
use serde_json::json;

use crate::support::BASE;
use crate::support::Synthetic;
use crate::support::TestResult;
use crate::support::instance;
use crate::support::package;

/// The vendored `hl7.fhir.r3.core` 3.0.2 package directory.
pub(crate) fn stu3_core() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/fhir-codegen/vendor/hl7.fhir.r3.core")
}

/// The synthetic STU3 profile package: `definitions` with the STU3 core
/// package beside it, read as STU3.
pub(crate) fn stu3_set(
    definitions: &[serde_json::Value],
) -> TestResult<(Synthetic, PackageSet<Stu3>)> {
    let synthetic = package(
        "example.stu3.profiles",
        "0.1.0",
        &[("hl7.fhir.r3.core", "3.0.2")],
        definitions,
    )?;
    let set = PackageSet::<Stu3>::load_release([stu3_core(), synthetic.path().to_owned()])?;
    Ok((synthetic, set))
}

/// An STU3 `StructureDefinition` constraining `kind` with the snapshot
/// `elements`.
pub(crate) fn stu3_profile(
    id: &str,
    kind: &str,
    elements: &serde_json::Value,
) -> serde_json::Value {
    json!({
        "resourceType": "StructureDefinition",
        "id": id,
        "url": format!("{BASE}{id}"),
        "version": "0.1.0",
        "name": id.replace('-', ""),
        "status": "draft",
        "kind": if kind == "Extension" || kind == "HumanName" { "complex-type" } else { "resource" },
        "abstract": false,
        "type": kind,
        "baseDefinition": format!("http://hl7.org/fhir/StructureDefinition/{kind}"),
        "derivation": "constraint",
        "snapshot": {"element": elements}
    })
}

/// Validates the STU3 `resource` against `profile`, or its core definition.
pub(crate) fn validate_stu3(
    set: &PackageSet<Stu3>,
    resource: &serde_json::Value,
    profile: Option<&str>,
) -> TestResult<Report> {
    let validator = Validator::new(set);
    let resource = instance(resource)?;
    Ok(match profile {
        Some(profile) => validator.validate_against(&resource, profile)?,
        None => validator.validate(&resource)?,
    })
}

/// The error and fatal issues of `report`, as message ids with locations.
pub(crate) fn errors(report: &Report) -> Vec<String> {
    report
        .issues()
        .iter()
        .filter(|issue| matches!(issue.severity, Severity::Error | Severity::Fatal))
        .map(|issue| {
            format!(
                "{:?} at {}: {}",
                issue.rule, issue.expression, issue.message
            )
        })
        .collect()
}
