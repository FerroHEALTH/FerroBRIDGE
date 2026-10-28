// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The crate's boundaries, read from the graph Cargo resolves (no
//! specification governs the layout: our own design).
//!
//! The FHIR R4 serialisation compiles no openEHR crate and no mapping engine
//! without the `openehr` feature, and no feature reaches a logging
//! component, so the two harmonised components of Regulation (EU) 2025/327
//! stay independent of each other (Art 2(2)(n) and (o)). The walk follows the
//! normal and build edges; dev-dependencies link into nothing that ships.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;
use std::process::Command;

/// This crate's package name.
const CRATE: &str = "eehrxf";

/// The prefix of every crate of the openEHR family, by crates.io name.
const OPENEHR: &str = "openehr-";

/// The mapping engine, which only the `openehr` feature may compile.
const MAPPING_ENGINE: [&str; 2] = ["fhirconnect", "openehr-mapping-core"];

/// The European logging software component and the audit crate beside it,
/// by crates.io name, which no feature of this crate may reach.
const LOGGING: [&str; 2] = ["ehds-logging", "ihe-atna"];

/// The package names in the normal and build closure of this crate with the
/// features `selection` names, as `cargo tree` prints them from the lockfile
/// and the registry already on disk.
fn closure(selection: &[&str]) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let output = Command::new(cargo)
        .args(["tree", "--frozen", "--manifest-path", manifest])
        .args(["--package", CRATE, "--edges", "normal,build"])
        .args(["--prefix", "none", "--format", "{p}"])
        .args(selection)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo tree {selection:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect())
}

#[test]
fn the_fhir_serialisation_alone_compiles_no_openehr() -> Result<(), Box<dyn Error>> {
    let reached = closure(&["--no-default-features", "--features", "fhir-r4"])?;
    assert!(
        reached.contains(CRATE) && reached.contains("fhir-types"),
        "cargo tree did not list the crate or its FHIR model, so its output was not read"
    );
    let breaches: Vec<&String> = reached
        .iter()
        .filter(|name| name.starts_with(OPENEHR) || MAPPING_ENGINE.contains(&name.as_str()))
        .collect();
    assert!(
        breaches.is_empty(),
        "fhir-r4 alone compiles openEHR, which only the openehr feature may: {breaches:?}"
    );
    Ok(())
}

#[test]
fn no_feature_reaches_a_logging_component() -> Result<(), Box<dyn Error>> {
    let reached = closure(&["--all-features"])?;
    assert!(
        reached.contains(CRATE) && reached.contains("fhirconnect"),
        "cargo tree did not list the crate or its mapping engine, so its output was not read"
    );
    let breaches: Vec<&str> = LOGGING
        .into_iter()
        .filter(|name| reached.contains(*name))
        .collect();
    assert!(
        breaches.is_empty(),
        "the interoperability component reaches a logging component (Art 2(2)(n), (o)): {breaches:?}"
    );
    Ok(())
}
