// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's crate boundary, read from the graph Cargo resolves: the
//! server half links `openehr-mapping-core` and `fhir-types` of the
//! workspace's own crates and no other, and the browser half links none
//! (docs/architecture.md section 4.10), so the console never compiles or
//! runs a mapping itself.
//!
//! The walk follows normal and build edges from the console through the
//! whole resolved graph, so a workspace crate reached through a third-party
//! crate counts as well; dev-dependencies link into no shipped artifact and
//! are not followed. The tests that inject an edge into the real graph show
//! the check refuses what it exists to refuse.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use serde_json::{Value, json};

/// The console's package name.
const VIEWER: &str = "ferrobridge-viewer";

/// The workspace crates the server half may link.
const SERVER_ALLOWED: [&str; 2] = ["openehr-mapping-core", "fhir-types"];

/// The target the browser half compiles for.
const BROWSER: &str = "wasm32-unknown-unknown";

/// The resolved graph `cargo metadata` answers for the workspace, every
/// target included or only `platform`'s.
fn metadata(platform: Option<&str>) -> Value {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let mut command = Command::new(cargo);
    command.args([
        "metadata",
        "--format-version",
        "1",
        "--locked",
        "--manifest-path",
        manifest,
    ]);
    if let Some(platform) = platform {
        command.args(["--filter-platform", platform]);
    }
    let output = command.output().expect("cargo metadata runs");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata answers JSON")
}

/// The package id of `name` among the workspace members.
fn member_id(metadata: &Value, name: &str) -> String {
    let members: BTreeSet<&str> = metadata["workspace_members"]
        .as_array()
        .expect("a member list")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    metadata["packages"]
        .as_array()
        .expect("a package list")
        .iter()
        .find(|package| {
            package["name"] == name && members.contains(package["id"].as_str().unwrap_or_default())
        })
        .and_then(|package| package["id"].as_str())
        .unwrap_or_else(|| panic!("{name} is a workspace member"))
        .to_owned()
}

/// Every package `root` reaches through normal and build edges, by name,
/// split into workspace members and the rest.
fn closure(metadata: &Value, root: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let names: BTreeMap<&str, &str> = metadata["packages"]
        .as_array()
        .expect("a package list")
        .iter()
        .filter_map(|package| Some((package["id"].as_str()?, package["name"].as_str()?)))
        .collect();
    let members: BTreeSet<&str> = metadata["workspace_members"]
        .as_array()
        .expect("a member list")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let nodes: BTreeMap<&str, &Value> = metadata["resolve"]["nodes"]
        .as_array()
        .expect("a resolved graph")
        .iter()
        .filter_map(|node| Some((node["id"].as_str()?, node)))
        .collect();
    let root = member_id(metadata, root);
    let mut seen = BTreeSet::from([root.clone()]);
    let mut queue = vec![root.clone()];
    while let Some(id) = queue.pop() {
        let node = nodes.get(id.as_str()).expect("every reached id is a node");
        for dep in node["deps"].as_array().expect("a dependency list") {
            let linked = dep["dep_kinds"]
                .as_array()
                .expect("dependency kinds")
                .iter()
                .any(|kind| kind["kind"] != "dev");
            let pkg = dep["pkg"].as_str().expect("a package id");
            if linked && seen.insert(pkg.to_owned()) {
                queue.push(pkg.to_owned());
            }
        }
    }
    seen.remove(&root);
    let (workspace, other): (Vec<&String>, Vec<&String>) =
        seen.iter().partition(|id| members.contains(id.as_str()));
    let name = |id: &String| names[id.as_str()].to_owned();
    (
        workspace.into_iter().map(name).collect(),
        other.into_iter().map(name).collect(),
    )
}

/// The workspace crates in `reached` outside `allowed`.
fn outside(reached: &BTreeSet<String>, allowed: &[&str]) -> BTreeSet<String> {
    reached
        .iter()
        .filter(|name| !allowed.contains(&name.as_str()))
        .cloned()
        .collect()
}

/// Adds an edge of `kind` (`null` for a normal one) from `from` to `to`,
/// both workspace members, to `metadata`'s resolved graph.
fn inject(metadata: &mut Value, from: &str, to: &str, kind: &Value) {
    let from = member_id(metadata, from);
    let to = member_id(metadata, to);
    let node = metadata["resolve"]["nodes"]
        .as_array_mut()
        .expect("a resolved graph")
        .iter_mut()
        .find(|node| node["id"] == from.as_str())
        .expect("the node of the edge's source");
    node["deps"]
        .as_array_mut()
        .expect("a dependency list")
        .push(json!({
            "name": "injected",
            "pkg": to,
            "dep_kinds": [{ "kind": kind.clone(), "target": null }],
        }));
}

#[test]
fn the_server_half_links_only_the_two_allowed_workspace_crates() {
    let metadata = metadata(None);
    let (workspace, _) = closure(&metadata, VIEWER);
    assert_eq!(
        BTreeSet::new(),
        outside(&workspace, &SERVER_ALLOWED),
        "the console links a workspace crate the boundary does not allow"
    );
    assert!(
        workspace.contains("fhir-types"),
        "the walk reaches the console's own dependency: {workspace:?}"
    );
}

#[test]
fn the_browser_half_links_no_workspace_crate() {
    let metadata = metadata(Some(BROWSER));
    let (workspace, other) = closure(&metadata, VIEWER);
    assert_eq!(
        BTreeSet::new(),
        workspace,
        "the browser half links a workspace crate"
    );
    assert!(
        other.contains("leptos") && other.contains("wasm-bindgen"),
        "the walk reaches the browser half's own dependencies: {other:?}"
    );
}

#[test]
fn an_injected_direct_dependency_on_another_workspace_crate_is_refused() {
    let mut metadata = metadata(None);
    inject(&mut metadata, VIEWER, "fhirconnect", &Value::Null);
    let (workspace, _) = closure(&metadata, VIEWER);
    assert!(
        outside(&workspace, &SERVER_ALLOWED).contains("fhirconnect"),
        "the check names the injected crate: {workspace:?}"
    );
}

#[test]
fn an_injected_dependency_reached_through_an_allowed_crate_is_refused() {
    let mut metadata = metadata(None);
    inject(
        &mut metadata,
        "fhir-types",
        "ferrobridge-term",
        &json!("build"),
    );
    let (workspace, _) = closure(&metadata, VIEWER);
    assert!(
        outside(&workspace, &SERVER_ALLOWED).contains("ferrobridge-term"),
        "the check follows the graph past an allowed crate: {workspace:?}"
    );
}

#[test]
fn an_injected_browser_dependency_is_refused() {
    let mut metadata = metadata(Some(BROWSER));
    inject(&mut metadata, VIEWER, "fhir-types", &Value::Null);
    let (workspace, _) = closure(&metadata, VIEWER);
    assert!(
        workspace.contains("fhir-types"),
        "the browser check names the injected crate: {workspace:?}"
    );
}

#[test]
fn a_dev_dependency_links_into_nothing_and_is_not_followed() {
    let mut metadata = metadata(None);
    inject(&mut metadata, VIEWER, "ferrobridge-testkit", &json!("dev"));
    let (workspace, _) = closure(&metadata, VIEWER);
    assert!(
        !workspace.contains("ferrobridge-testkit"),
        "a dev-dependency is outside the shipped closure: {workspace:?}"
    );
}
