// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OMOCL files `etl run` reads: `[mappings] omocl` and the console's
//! OMOCL files of the running revision in the state store, one owner per
//! path. The cases that need the store run only when `FERROBRIDGE_E2E=1`
//! admits the harness. No specification governs where the files live: our
//! own design.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ferrobridge_operator_api::revisions::Origin;
use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::etl::job::{JobError, has_omocl, missing_section, omocl_set};
use ferrobridge_server::etl::mapper::MapperError;
use ferrobridge_server::mappings::revisions::{Entry, Files, Recording, Revisions};
use ferrobridge_server::store::Store;

/// The OMOCL corpus cluster files the cases copy.
const CLUSTERS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/omocl/medical_data/cluster"
);

/// The smallest `[auth]` section the operator API boots beside.
const AUTH: &str = "[auth]\nenabled = true\n\n[auth.issuer.ferro]\nissuer = \"https://as.example.org/realms/ferro\"\naudience = [\"https://bridge.example.org/fhir\"]\n";

/// A provisioned OMOCL tree.
struct Tree {
    /// The root, kept alive for the case.
    root: tempfile::TempDir,
}

impl Tree {
    /// Creates the tree, empty.
    fn new() -> Result<Self, Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("omocl"))?;
        Ok(Self { root })
    }

    /// Returns the provisioned OMOCL directory.
    fn provisioned(&self) -> PathBuf {
        self.root.path().join("omocl")
    }

    /// Copies the corpus cluster `name` into the tree as `as_name`.
    fn copy(&self, name: &str, as_name: &str) -> Result<(), Box<dyn Error>> {
        std::fs::copy(
            Path::new(CLUSTERS).join(name),
            self.provisioned().join(as_name),
        )?;
        Ok(())
    }

    /// Returns the settings of a configuration naming the tree, when
    /// `provisioned`, and the operator API saving into the store at `url`.
    fn settings(
        &self,
        provisioned: bool,
        url: &str,
    ) -> Result<ferrobridge_server::config::Settings, Box<dyn Error>> {
        let mappings = if provisioned {
            format!(
                "[mappings]\nomocl = \"{}\"\n\n",
                self.provisioned().display()
            )
        } else {
            String::new()
        };
        let text =
            format!("{AUTH}\n{mappings}[operator]\nenabled = true\n\n[state]\nurl = \"{url}\"\n");
        Ok(Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?)
    }
}

/// Records `files`, each a wire path and a corpus cluster file or a text,
/// as the running revision of `store`.
async fn save(store: &Store, files: &[(&str, String)]) -> Result<(), Box<dyn Error>> {
    let files: Files = files
        .iter()
        .map(|(wire, text)| ((*wire).to_owned(), Arc::from(text.as_bytes())))
        .collect();
    let entry = Entry {
        origin: Origin::Save,
        author: String::from("synthetic-client-sub"),
        reason: String::from("synthetic reason"),
        source: None,
    };
    Revisions::new(store)
        .change()
        .await?
        .record(entry, &files, Recording::Run)
        .await?;
    Ok(())
}

/// Returns the text of the corpus cluster `name`.
fn cluster(name: &str) -> Result<String, Box<dyn Error>> {
    Ok(std::fs::read_to_string(Path::new(CLUSTERS).join(name))?)
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
fn without_the_operator_api_and_the_store_only_the_provisioned_tree_counts()
-> Result<(), Box<dyn Error>> {
    let tree = Tree::new()?;
    let text = format!("[mappings]\nomocl = \"{}\"\n", tree.provisioned().display());
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    assert!(has_omocl(&settings));
    let bare = Config::from_sources(Some(""), &BTreeMap::new())?.resolve(Process::Server)?;
    assert!(!has_omocl(&bare));
    assert_eq!(Some("an [etl] section"), missing_section(&bare));
    Ok(())
}

#[tokio::test]
async fn etl_run_maps_with_the_provisioned_and_the_console_files_together()
-> Result<(), Box<dyn Error>> {
    let Some((postgres, store)) = crate::store::store().await? else {
        return Ok(());
    };
    let tree = Tree::new()?;
    tree.copy(
        "Laboratory_test_analyte_v1.yml",
        "Laboratory_test_analyte_v1.yml",
    )?;
    save(
        &store,
        &[("omocl/Specimen_v1.yml", cluster("Specimen_v1.yml")?)],
    )
    .await?;
    let set = omocl_set(&tree.settings(true, postgres.url())?, Some(&store)).await?;
    assert_eq!(
        vec![
            String::from("Laboratory_test_analyte_v1"),
            String::from("Specimen_v1")
        ],
        names(&set)
    );
    Ok(())
}

#[tokio::test]
async fn a_provisioned_file_owns_a_path_the_console_also_holds() -> Result<(), Box<dyn Error>> {
    let Some((postgres, store)) = crate::store::store().await? else {
        return Ok(());
    };
    let tree = Tree::new()?;
    tree.copy("Specimen_v1.yml", "Specimen_v1.yml")?;
    save(
        &store,
        &[(
            "omocl/Specimen_v1.yml",
            String::from("grammar: [unclosed\n"),
        )],
    )
    .await?;
    let set = omocl_set(&tree.settings(true, postgres.url())?, Some(&store)).await?;
    assert_eq!(vec![String::from("Specimen_v1")], names(&set));
    Ok(())
}

#[tokio::test]
async fn a_console_file_defining_a_provisioned_mapping_is_refused() -> Result<(), Box<dyn Error>> {
    let Some((postgres, store)) = crate::store::store().await? else {
        return Ok(());
    };
    let tree = Tree::new()?;
    tree.copy("Specimen_v1.yml", "Specimen_v1.yml")?;
    save(
        &store,
        &[("omocl/specimen_copy.yml", cluster("Specimen_v1.yml")?)],
    )
    .await?;
    let refused = omocl_set(&tree.settings(true, postgres.url())?, Some(&store)).await;
    assert!(
        matches!(
            refused,
            Err(JobError::Mappings(MapperError::Mappings { .. }))
        ),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_console_files_alone_serve_and_none_is_refused() -> Result<(), Box<dyn Error>> {
    let Some((postgres, store)) = crate::store::store().await? else {
        return Ok(());
    };
    let tree = Tree::new()?;
    let settings = tree.settings(false, postgres.url())?;
    assert!(has_omocl(&settings));
    let empty = omocl_set(&settings, None).await;
    assert!(
        matches!(empty, Err(JobError::Mappings(MapperError::Empty { .. }))),
        "a store with no revision holds no file: {empty:?}"
    );
    save(
        &store,
        &[("omocl/Specimen_v1.yml", cluster("Specimen_v1.yml")?)],
    )
    .await?;
    assert_eq!(
        vec![String::from("Specimen_v1")],
        names(&omocl_set(&settings, None).await?)
    );
    Ok(())
}
