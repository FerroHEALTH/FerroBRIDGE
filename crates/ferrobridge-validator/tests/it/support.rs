// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Synthetic packages and profiles, written into temporary directories, and
//! the vendored R4 core package they depend on.

use std::error::Error;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use ferrobridge_validator::issue::Issue;
use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Report;
use ferrobridge_validator::set::PackageSet;
use ferrobridge_validator::validate::Validator;
use fhir_types::codec::Value;
use serde_json::json;

/// The result type of the tests.
pub(crate) type TestResult<T = ()> = Result<T, Box<dyn Error>>;

/// The canonical base of the synthetic definitions.
pub(crate) const BASE: &str = "http://example.org/fhir/StructureDefinition/";

/// The vendored `hl7.fhir.r4.core` 4.0.1 package directory.
pub(crate) fn core() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/fhir-codegen/vendor/hl7.fhir.r4.core")
}

/// Returns the directory `scripts/vendor/profile-packages.sh` fetches the
/// profile packages into, `None` when they are absent.
///
/// Under CI (`CI=true`) an absent tree is an error, so a lane cannot pass by
/// skipping the package-gated tests.
pub(crate) fn profile_packages() -> TestResult<Option<PathBuf>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/specs/fhir-packages");
    if root
        .join("hl7.fhir.eu.base#2.0.1/package/package.json")
        .is_file()
    {
        return Ok(Some(root));
    }
    if std::env::var("CI").is_ok_and(|value| value == "true") {
        return Err(format!(
            "the profile packages are absent from {} under CI; run scripts/vendor/profile-packages.sh before the tests",
            root.display()
        )
        .into());
    }
    Ok(None)
}

/// A synthetic package written to a temporary directory.
pub(crate) struct Synthetic {
    pub(crate) dir: tempfile::TempDir,
}

impl Synthetic {
    /// The directory holding `package/`.
    pub(crate) fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// Writes a package named `name#version` depending on `dependencies`, with
/// one file per resource, named `<resourceType>-<id>.json`.
pub(crate) fn package(
    name: &str,
    version: &str,
    dependencies: &[(&str, &str)],
    resources: &[serde_json::Value],
) -> TestResult<Synthetic> {
    let dir = tempfile::tempdir()?;
    let folder = dir.path().join("package");
    fs::create_dir_all(&folder)?;
    let dependencies: serde_json::Map<String, serde_json::Value> = dependencies
        .iter()
        .map(|(name, version)| ((*name).to_owned(), json!(version)))
        .collect();
    let manifest = json!({"name": name, "version": version, "dependencies": dependencies});
    fs::write(
        folder.join("package.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    for resource in resources {
        let file = format!(
            "{}-{}.json",
            resource["resourceType"].as_str().unwrap_or("Resource"),
            resource["id"].as_str().unwrap_or("x")
        );
        fs::write(folder.join(file), serde_json::to_vec_pretty(resource)?)?;
    }
    Ok(Synthetic { dir })
}

/// Writes the `.tgz` form of the package at `dir` to `out`.
pub(crate) fn tarball(dir: &Path, out: &Path) -> TestResult {
    let file = fs::File::create(out)?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    builder.append_dir_all("package", dir.join("package"))?;
    builder.into_inner()?.finish()?;
    Ok(())
}

/// The synthetic profile package: the definitions `definitions` with the R4
/// core package beside it.
pub(crate) fn set_with(definitions: &[serde_json::Value]) -> TestResult<(Synthetic, PackageSet)> {
    let synthetic = package(
        "example.profiles",
        "0.1.0",
        &[("hl7.fhir.r4.core", "4.0.1")],
        definitions,
    )?;
    let set = PackageSet::load([core(), synthetic.path().to_owned()])?;
    Ok((synthetic, set))
}

/// A `StructureDefinition` constraining `kind` with the snapshot `elements`.
///
/// The snapshot is partial: the validator holds an instance to the elements a
/// snapshot carries, and the element table supplies the base of the rest.
pub(crate) fn profile(id: &str, kind: &str, elements: &serde_json::Value) -> serde_json::Value {
    json!({
        "resourceType": "StructureDefinition",
        "id": id,
        "url": format!("{BASE}{id}"),
        "version": "0.1.0",
        "name": id.replace('-', ""),
        "status": "draft",
        "kind": if kind == "Extension" { "complex-type" } else { "resource" },
        "abstract": false,
        "type": kind,
        "baseDefinition": format!("http://hl7.org/fhir/StructureDefinition/{kind}"),
        "derivation": "constraint",
        "snapshot": {"element": elements}
    })
}

/// A simple extension whose value SHALL be of `value_type`.
pub(crate) fn extension(id: &str, value_type: &str, modifier: bool) -> serde_json::Value {
    let mut definition = profile(
        id,
        "Extension",
        &json!([
            {"id": "Extension", "path": "Extension", "min": 0, "max": "*", "isModifier": modifier},
            {"id": "Extension.extension", "path": "Extension.extension", "min": 0, "max": "0"},
            {"id": "Extension.url", "path": "Extension.url", "min": 1, "max": "1",
             "type": [{"code": "uri"}], "fixedUri": format!("{BASE}{id}")},
            {"id": "Extension.value[x]", "path": "Extension.value[x]", "min": 1, "max": "1",
             "type": [{"code": value_type}]}
        ]),
    );
    definition["context"] = json!([{"type": "element", "expression": "Element"}]);
    definition
}

/// The canonical of [`patient_profile`].
pub(crate) fn patient_url() -> String {
    format!("{BASE}patient-test")
}

/// A synthetic Patient profile: one or two names, each with a family name
/// of at most five characters; the gender fixed to `female`; the marital
/// status holding a pattern coding; `deceased[x]` restricted to `boolean`;
/// the identifiers sliced.
pub(crate) fn patient_profile() -> serde_json::Value {
    profile(
        "patient-test",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.identifier", "path": "Patient.identifier", "min": 0, "max": "*",
             "slicing": {"discriminator": [{"type": "value", "path": "system"}], "rules": "open"},
             "type": [{"code": "Identifier"}]},
            {"id": "Patient.identifier:local", "path": "Patient.identifier", "sliceName": "local",
             "min": 0, "max": "1", "type": [{"code": "Identifier"}]},
            {"id": "Patient.name", "path": "Patient.name", "min": 1, "max": "2",
             "type": [{"code": "HumanName"}]},
            {"id": "Patient.name.family", "path": "Patient.name.family", "min": 1, "max": "1",
             "type": [{"code": "string"}], "maxLength": 5},
            {"id": "Patient.gender", "path": "Patient.gender", "min": 0, "max": "1",
             "type": [{"code": "code"}], "fixedCode": "female"},
            {"id": "Patient.maritalStatus", "path": "Patient.maritalStatus", "min": 0, "max": "1",
             "type": [{"code": "CodeableConcept"}],
             "patternCodeableConcept": {"coding": [
                 {"system": "http://terminology.hl7.org/CodeSystem/v3-MaritalStatus", "code": "M"}
             ]}},
            {"id": "Patient.deceased[x]", "path": "Patient.deceased[x]", "min": 0, "max": "1",
             "type": [{"code": "boolean"}]}
        ]),
    )
}

/// A synthetic Patient that conforms to [`patient_profile`].
pub(crate) fn patient() -> serde_json::Value {
    json!({
        "resourceType": "Patient",
        "name": [{"family": "Doe", "given": ["Ada"]}],
        "gender": "female",
        "maritalStatus": {"coding": [
            {"system": "http://terminology.hl7.org/CodeSystem/v3-MaritalStatus", "code": "M", "display": "Married"}
        ]},
        "deceasedBoolean": false
    })
}

/// The patient profile's package set: the profile and the extensions
/// `flag` (boolean), `label` (string) and `stop` (a boolean modifier).
pub(crate) fn patient_set() -> TestResult<(Synthetic, PackageSet)> {
    set_with(&[
        patient_profile(),
        extension("flag", "boolean", false),
        extension("label", "string", false),
        extension("stop", "boolean", true),
    ])
}

/// Validates `resource` against [`patient_profile`].
pub(crate) fn against_patient(resource: &serde_json::Value) -> TestResult<Report> {
    let (_synthetic, set) = patient_set()?;
    validate(&set, resource, Some(&patient_url()))
}

/// Parses `value` into the fhir-types document model.
pub(crate) fn instance(value: &serde_json::Value) -> TestResult<Value> {
    Ok(serde_json::from_str(&serde_json::to_string(value)?)?)
}

/// Validates `resource` against `profile`, or its core definition.
pub(crate) fn validate(
    set: &PackageSet,
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

/// The issues of `report` carrying `id`.
pub(crate) fn issues(report: &Report, id: MessageId) -> Vec<&Issue> {
    report.with_message(id).collect()
}

/// The locations of the issues of `report` carrying `id`.
pub(crate) fn locations(report: &Report, id: MessageId) -> Vec<String> {
    report
        .with_message(id)
        .map(|issue| issue.expression.clone())
        .collect()
}
