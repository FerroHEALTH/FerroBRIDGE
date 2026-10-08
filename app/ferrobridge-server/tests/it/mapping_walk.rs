// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping directory walk over a Kubernetes `ConfigMap` volume.
//!
//! The volume keeps its files in a hidden `..<timestamp>` directory, points a
//! `..data` symlink at it, and exposes each key as a top-level file symlink
//! through `..data`. The walk reads each mapping once. No specification
//! governs this: our own design.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::path::PathBuf;

use ferrobridge_server::config::Config;
use ferrobridge_server::state::AppState;
use ferrobridge_server::walk;

/// The synthetic mapping set the suite lays out as a `ConfigMap` volume.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures");

/// The mapping files of the fixture set, one `ConfigMap` key each.
const KEYS: [&str; 2] = ["ferrobridge_facade.yml", "ferrobridge_facade.context.yml"];

/// Lays the fixture mapping set out under `volume` the way the kubelet
/// writes a `ConfigMap` volume.
fn configmap_volume(volume: &Path) -> Result<(), Box<dyn StdError>> {
    let timestamped = "..2026_10_08_12_00_00.000000001";
    std::fs::create_dir_all(volume.join(timestamped))?;
    for key in KEYS {
        std::fs::copy(
            Path::new(FIXTURES).join(key),
            volume.join(timestamped).join(key),
        )?;
    }
    symlink(timestamped, volume.join("..data"))?;
    for key in KEYS {
        symlink(Path::new("..data").join(key), volume.join(key))?;
    }
    Ok(())
}

/// Returns every YAML file the walk finds under `directory`.
fn yaml_files(directory: &Path) -> Result<Vec<PathBuf>, walk::Error> {
    walk::files(directory, |path| {
        walk::has_extension(path, &["yml", "yaml"])
    })
}

#[test]
fn a_configmap_volume_lists_each_mapping_once() -> Result<(), Box<dyn StdError>> {
    let volume = tempfile::tempdir()?;
    configmap_volume(volume.path())?;

    let mut expected: Vec<PathBuf> = KEYS.iter().map(|key| volume.path().join(key)).collect();
    expected.sort();
    assert_eq!(expected, yaml_files(volume.path())?);
    Ok(())
}

#[test]
fn a_symlink_to_a_directory_is_not_followed() -> Result<(), Box<dyn StdError>> {
    let elsewhere = tempfile::tempdir()?;
    std::fs::copy(
        Path::new(FIXTURES).join("ferrobridge_facade.yml"),
        elsewhere.path().join("elsewhere.yml"),
    )?;
    let directory = tempfile::tempdir()?;
    symlink(elsewhere.path(), directory.path().join("linked"))?;

    assert_eq!(Vec::<PathBuf>::new(), yaml_files(directory.path())?);
    Ok(())
}

#[test]
fn a_real_subdirectory_is_walked() -> Result<(), Box<dyn StdError>> {
    let directory = tempfile::tempdir()?;
    std::fs::create_dir_all(directory.path().join("nested"))?;
    let nested = directory.path().join("nested").join("x.yaml");
    std::fs::write(&nested, "synthetic: true\n")?;

    assert_eq!(vec![nested], yaml_files(directory.path())?);
    Ok(())
}

#[test]
fn a_dangling_file_symlink_is_refused_naming_it() -> Result<(), Box<dyn StdError>> {
    let directory = tempfile::tempdir()?;
    let dangling = directory.path().join("gone.yaml");
    symlink(directory.path().join("absent.yaml"), &dangling)?;

    let error = yaml_files(directory.path())
        .err()
        .ok_or("a dangling symlink is refused")?;
    assert_eq!(dangling, error.path);
    Ok(())
}

#[tokio::test]
async fn a_configmap_volume_mapping_directory_boots_the_operations_lane()
-> Result<(), Box<dyn StdError>> {
    let volume = tempfile::tempdir()?;
    configmap_volume(volume.path())?;
    let templates = tempfile::tempdir()?;
    std::fs::write(
        templates.path().join("diagnose.opt"),
        ferrobridge_testkit::fixtures::DIAGNOSE_OPT,
    )?;
    let text = format!(
        "[mappings]\ndirectory = \"{}\"\ntemplates = \"{}\"\n",
        volume.path().display(),
        templates.path().display()
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;

    let state = AppState::build(&settings).await?;
    assert_eq!(
        1,
        state
            .operations()
            .ok_or("the operations lane is served")?
            .programs()
            .len()
    );
    Ok(())
}
