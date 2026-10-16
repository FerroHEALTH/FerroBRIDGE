// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `terminology` command reads every JSON file of a package: a file that
//! is not a resource is told apart by its shape, and a file that does not
//! parse fails the run with its path.

use std::fs;
use std::path::Path;

use fhir_codegen::emit::VersionInput;
use fhir_codegen::terminology::{BundleError, BundleOptions, bundle};

/// A one-version package under `root` holding `files`, with the options that
/// bundle it into `root/out`.
fn package(root: &Path, files: &[(&str, &str)]) -> BundleOptions {
    let dir = root.join("hl7.fhir.example").join("package");
    fs::create_dir_all(&dir).expect("the package directory is created");
    for (name, text) in files {
        fs::write(dir.join(name), text).expect("the package file is written");
    }
    BundleOptions {
        versions: vec![VersionInput {
            module: "r4".to_owned(),
            package_dir: root.join("hl7.fhir.example"),
        }],
        data_dir: root.join("out"),
        check: false,
    }
}

const CODE_SYSTEM: &str = r#"{"resourceType":"CodeSystem","url":"http://hl7.org/fhir/example","status":"active","content":"complete","concept":[{"code":"a"}]}"#;

#[test]
fn a_manifest_and_an_index_are_not_resources() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let options = package(
        root.path(),
        &[
            (
                "package.json",
                r#"{"name":"hl7.fhir.example","version":"0.0.1"}"#,
            ),
            (".index.json", r#"{"index-version":1,"files":[]}"#),
            ("CodeSystem-example.json", CODE_SYSTEM),
        ],
    );
    let report = bundle(&options).expect("the package bundles");
    assert_eq!(report.counts.get("r4").map(|c| c.code_systems), Some(1));
}

#[test]
fn an_unparsable_package_file_fails_the_run_with_its_path() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let options = package(
        root.path(),
        &[
            ("CodeSystem-example.json", CODE_SYSTEM),
            ("ValueSet-broken.json", r#"{"resourceType":"ValueSet","#),
        ],
    );
    let error = bundle(&options).expect_err("the defective file fails the run");
    let BundleError::Json { path, .. } = &error else {
        panic!("expected a JSON failure, got {error}");
    };
    assert_eq!(
        path.file_name().and_then(std::ffi::OsStr::to_str),
        Some("ValueSet-broken.json")
    );
    assert!(
        error.to_string().contains("ValueSet-broken.json"),
        "{error}"
    );
}

#[test]
fn a_leading_byte_order_mark_is_not_a_parse_failure() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let marked = format!("\u{feff}{CODE_SYSTEM}");
    let options = package(root.path(), &[("CodeSystem-example.json", &marked)]);
    let report = bundle(&options).expect("the package bundles");
    assert_eq!(report.counts.get("r4").map(|c| c.code_systems), Some(1));
}
