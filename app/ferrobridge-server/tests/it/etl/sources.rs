// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OMOCL files `etl run` reads: `[mappings] omocl` and the console's
//! `omocl/` under `[operator] mapping_dir`, one owner per path. No
//! specification governs where the files live: our own design.

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::path::{Path, PathBuf};

use ferrobridge_server::config::Config;
use ferrobridge_server::etl::job::{missing_section, omocl_roots};
use ferrobridge_server::etl::mapper::{MapperError, read_roots};
use ferrobridge_server::operator::files::{Language, Owner};

/// The OMOCL corpus cluster files the cases copy.
const CLUSTERS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/omocl/medical_data/cluster"
);

/// The smallest `[auth]` section the operator API boots beside.
const AUTH: &str = "[auth]\nenabled = true\nissuer = \"https://as.example.org/realms/ferro\"\naudience = [\"https://bridge.example.org/fhir\"]\n";

/// A provisioned OMOCL tree and a console tree under one temporary root.
struct Trees {
    /// The root, kept alive for the case.
    root: tempfile::TempDir,
}

impl Trees {
    /// Creates both trees, empty.
    fn new() -> Result<Self, Box<dyn StdError>> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("omocl"))?;
        std::fs::create_dir_all(root.path().join("console/omocl"))?;
        Ok(Self { root })
    }

    /// Returns the provisioned OMOCL directory.
    fn provisioned(&self) -> PathBuf {
        self.root.path().join("omocl")
    }

    /// Returns the console's `[operator] mapping_dir`.
    fn mapping_dir(&self) -> PathBuf {
        self.root.path().join("console")
    }

    /// Copies the corpus cluster `name` to `directory` as `as_name`.
    fn copy(directory: &Path, name: &str, as_name: &str) -> Result<(), Box<dyn StdError>> {
        std::fs::copy(Path::new(CLUSTERS).join(name), directory.join(as_name))?;
        Ok(())
    }

    /// Returns the settings of a configuration naming both trees.
    fn settings(&self) -> Result<ferrobridge_server::config::Settings, Box<dyn StdError>> {
        let text = format!(
            "{AUTH}\n[mappings]\nomocl = \"{}\"\n\n[operator]\nenabled = true\nmapping_dir = \"{}\"\n",
            self.provisioned().display(),
            self.mapping_dir().display()
        );
        Ok(Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?)
    }
}

/// Returns the mapping names of `set`, sorted.
fn names(set: &omocl::model::load::MappingSet) -> Vec<String> {
    let mut names: Vec<String> = set
        .files()
        .map(|file| file.header().name().value().as_str().to_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn the_console_tree_joins_the_provisioned_one_when_the_operator_api_saves()
-> Result<(), Box<dyn StdError>> {
    let trees = Trees::new()?;
    let roots = omocl_roots(&trees.settings()?);
    let owners: Vec<(Language, Owner, &Path)> = roots
        .iter()
        .map(|root| (root.language(), root.owner(), root.directory()))
        .collect();
    let console = trees.mapping_dir().join("omocl");
    let provisioned = trees.provisioned();
    assert_eq!(
        vec![
            (Language::Omocl, Owner::Provisioned, provisioned.as_path()),
            (Language::Omocl, Owner::Console, console.as_path()),
        ],
        owners
    );
    Ok(())
}

#[test]
fn without_the_operator_api_only_the_provisioned_tree_is_read() -> Result<(), Box<dyn StdError>> {
    let trees = Trees::new()?;
    let text = format!(
        "[mappings]\nomocl = \"{}\"\n",
        trees.provisioned().display()
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let roots = omocl_roots(&settings);
    assert_eq!(1, roots.len());
    assert_eq!(
        Some(Owner::Provisioned),
        roots
            .first()
            .map(ferrobridge_server::operator::files::Root::owner)
    );
    let bare = Config::from_sources(Some(""), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        omocl_roots(&bare),
        Vec::<ferrobridge_server::operator::files::Root>::new()
    );
    assert_eq!(Some("an [etl] section"), missing_section(&bare));
    Ok(())
}

#[test]
fn etl_run_maps_with_the_provisioned_and_the_console_files_together()
-> Result<(), Box<dyn StdError>> {
    let trees = Trees::new()?;
    Trees::copy(
        &trees.provisioned(),
        "Laboratory_test_analyte_v1.yml",
        "Laboratory_test_analyte_v1.yml",
    )?;
    Trees::copy(
        &trees.mapping_dir().join("omocl"),
        "Specimen_v1.yml",
        "Specimen_v1.yml",
    )?;
    let set = read_roots(&omocl_roots(&trees.settings()?))?;
    assert_eq!(
        vec![
            String::from("Laboratory_test_analyte_v1"),
            String::from("Specimen_v1")
        ],
        names(&set)
    );
    Ok(())
}

#[test]
fn a_provisioned_file_owns_a_path_the_console_tree_also_holds() -> Result<(), Box<dyn StdError>> {
    let trees = Trees::new()?;
    Trees::copy(&trees.provisioned(), "Specimen_v1.yml", "Specimen_v1.yml")?;
    std::fs::write(
        trees.mapping_dir().join("omocl/Specimen_v1.yml"),
        "grammar: [unclosed\n",
    )?;
    let set = read_roots(&omocl_roots(&trees.settings()?))?;
    assert_eq!(vec![String::from("Specimen_v1")], names(&set));
    Ok(())
}

#[test]
fn a_console_file_defining_a_provisioned_mapping_is_refused() -> Result<(), Box<dyn StdError>> {
    let trees = Trees::new()?;
    Trees::copy(&trees.provisioned(), "Specimen_v1.yml", "Specimen_v1.yml")?;
    Trees::copy(
        &trees.mapping_dir().join("omocl"),
        "Specimen_v1.yml",
        "specimen_copy.yml",
    )?;
    let refused = read_roots(&omocl_roots(&trees.settings()?));
    assert!(
        matches!(refused, Err(MapperError::Mappings { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_console_tree_alone_serves_and_an_empty_one_is_refused() -> Result<(), Box<dyn StdError>> {
    let trees = Trees::new()?;
    let text = format!(
        "{AUTH}\n[operator]\nenabled = true\nmapping_dir = \"{}\"\n",
        trees.mapping_dir().display()
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let roots = omocl_roots(&settings);
    assert!(
        matches!(read_roots(&roots), Err(MapperError::Empty { .. })),
        "an empty console tree holds no file"
    );
    Trees::copy(
        &trees.mapping_dir().join("omocl"),
        "Specimen_v1.yml",
        "Specimen_v1.yml",
    )?;
    assert_eq!(
        vec![String::from("Specimen_v1")],
        names(&read_roots(&roots)?)
    );
    Ok(())
}
