// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The boundary between the logging component and the interoperability
//! component, read from the graph Cargo resolves with every feature on.
//!
//! Regulation (EU) 2025/327 Art 2(2)(n) and (o) define the two components of
//! an EHR system as independent of each other, and Art 30(1)(b) has the
//! manufacturer keep each unaffected by other software components. So
//! `ehds-logging` and `ihe-atna` are linked by the server alone, and none of
//! `eehrxf`, `fhirconnect` and `openehr-mapping-core` reaches either through
//! a normal or a build edge. The injected edge shows the check refuses what
//! it exists to refuse.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use serde_json::{Value, json};

/// The audit crates.
const AUDIT: [&str; 2] = ["ehds-logging", "ihe-atna"];

/// The crates of the interoperability component that may reach neither.
const INTEROPERABILITY: [&str; 3] = ["eehrxf", "fhirconnect", "openehr-mapping-core"];

/// The one workspace member that links them.
const SERVER: &str = "ferrobridge-server";

/// The resolved graph `cargo metadata` answers with every feature on.
fn metadata() -> Value {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--all-features",
            "--manifest-path",
            manifest,
        ])
        .output()
        .expect("cargo metadata runs");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata answers JSON")
}

/// The package names by id, and the workspace members' ids.
fn names(metadata: &Value) -> (BTreeMap<String, String>, BTreeSet<String>) {
    let names = metadata["packages"]
        .as_array()
        .expect("a package list")
        .iter()
        .filter_map(|package| {
            Some((
                package["id"].as_str()?.to_owned(),
                package["name"].as_str()?.to_owned(),
            ))
        })
        .collect();
    let members = metadata["workspace_members"]
        .as_array()
        .expect("a member list")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    (names, members)
}

/// The id of the workspace member `name`.
fn member(metadata: &Value, name: &str) -> String {
    let (names, members) = names(metadata);
    members
        .into_iter()
        .find(|id| names[id] == name)
        .expect("the named crate is a workspace member")
}

/// The ids `id` links through a normal or a build edge.
fn linked(metadata: &Value, id: &str) -> Vec<String> {
    metadata["resolve"]["nodes"]
        .as_array()
        .expect("a resolved graph")
        .iter()
        .find(|node| node["id"] == id)
        .and_then(|node| node["deps"].as_array())
        .map(|deps| {
            deps.iter()
                .filter(|dep| {
                    dep["dep_kinds"]
                        .as_array()
                        .is_some_and(|kinds| kinds.iter().any(|kind| kind["kind"] != "dev"))
                })
                .filter_map(|dep| dep["pkg"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Every package name the member `root` reaches through normal and build
/// edges.
fn closure(metadata: &Value, root: &str) -> BTreeSet<String> {
    let (names, _) = names(metadata);
    let root = member(metadata, root);
    let mut seen = BTreeSet::from([root.clone()]);
    let mut queue = vec![root.clone()];
    while let Some(id) = queue.pop() {
        for pkg in linked(metadata, &id) {
            if seen.insert(pkg.clone()) {
                queue.push(pkg);
            }
        }
    }
    seen.remove(&root);
    seen.iter().map(|id| names[id].clone()).collect()
}

/// The workspace members that link an audit crate directly.
fn linking_members(metadata: &Value) -> BTreeSet<String> {
    let (names, members) = names(metadata);
    members
        .into_iter()
        .filter(|id| {
            linked(metadata, id)
                .iter()
                .any(|pkg| AUDIT.contains(&names[pkg].as_str()))
        })
        .map(|id| names[&id].clone())
        .collect()
}

/// The audit crates `root` reaches.
fn reached_audit(metadata: &Value, root: &str) -> BTreeSet<String> {
    closure(metadata, root)
        .into_iter()
        .filter(|name| AUDIT.contains(&name.as_str()))
        .collect()
}

#[test]
fn no_interoperability_crate_reaches_an_audit_crate() {
    let metadata = metadata();
    for root in INTEROPERABILITY {
        assert_eq!(
            BTreeSet::<String>::new(),
            reached_audit(&metadata, root),
            "{root} reaches the logging component"
        );
    }
}

#[test]
fn only_the_server_links_the_audit_crates() {
    let metadata = metadata();
    assert_eq!(
        BTreeSet::from([SERVER.to_owned()]),
        linking_members(&metadata)
    );
}

#[test]
fn an_edge_from_an_interoperability_crate_to_an_audit_crate_is_refused() {
    let mut metadata = metadata();
    let (names, _) = names(&metadata);
    let audit = names
        .iter()
        .find(|(_, name)| name.as_str() == "ehds-logging")
        .map(|(id, _)| id.clone())
        .expect("the graph resolves ehds-logging");
    let from = member(&metadata, "fhirconnect");
    let node = metadata["resolve"]["nodes"]
        .as_array_mut()
        .expect("a resolved graph")
        .iter_mut()
        .find(|node| node["id"] == from.as_str())
        .expect("the node of fhirconnect");
    node["deps"]
        .as_array_mut()
        .expect("a dependency list")
        .push(json!({
            "name": "injected",
            "pkg": audit,
            "dep_kinds": [{ "kind": null, "target": null }],
        }));
    assert_eq!(
        BTreeSet::from([String::from("ehds-logging"), String::from("ihe-atna")]),
        reached_audit(&metadata, "fhirconnect")
    );
    assert!(linking_members(&metadata).contains("fhirconnect"));
}
