// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading the fetched HL7 v2 definitions from disk.
//!
//! The v2ig source of truth is a repository directory of bare
//! `StructureDefinition` files with no `package.json`, so
//! [`crate::package::Package`] does not open it. The loader reads the
//! directories [`crate::roots`] declares, indexes each definition by canonical
//! URL in ordered maps, and keeps each file's path relative to the tree so a
//! defect is reported, and tolerated, by the file that carries it.

use std::collections::BTreeMap;
use std::path::Path;

use crate::roots::{V2_BASES, V2_DATA_TYPE_DIRS, V2_MESSAGE_DIR, V2_SEGMENT_DIR, V2_STRUCTURE_DIR};
use crate::v2::definition::{MessageDefinition, StructureDefinition};
use crate::v2::fetched;
use crate::v2::fetched::{FetchError, Fetched, Source};

/// The directory under the fetched tree that holds the definitions.
pub const SOURCE_OF_TRUTH: &str = "input/sourceOfTruth";

/// The fetched HL7 v2 definitions.
const DEFINITIONS: Source = Source {
    dir: SOURCE_OF_TRUTH,
    holds: "definitions",
    script: "scripts/vendor/v2ig.sh",
};

/// A failure while reading the v2 definitions.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The fetched tree did not open, or a file in it did not read or parse.
    #[error(transparent)]
    Fetched(#[from] FetchError),
    /// A file in a definition directory is not a `StructureDefinition`.
    #[error("{path} is a {resource_type}, not a StructureDefinition")]
    NotAStructure {
        /// The file.
        path: String,
        /// The resource type it declares.
        resource_type: String,
    },
    /// Two files define one canonical URL.
    #[error("{url} is defined twice: {first} and {second}")]
    DuplicateCanonical {
        /// The duplicated canonical URL.
        url: String,
        /// The file first defining it.
        first: String,
        /// The file defining it again.
        second: String,
    },
}

/// One definition and the file it came from, relative to the definitions tree.
#[derive(Debug, Clone)]
pub struct Sourced {
    /// The file, for example `segment/segments/OBX.json`.
    pub file: String,
    /// The definition.
    pub definition: StructureDefinition,
}

/// One message definition and the file it came from, relative to the
/// definitions tree.
#[derive(Debug, Clone)]
pub struct SourcedMessage {
    /// The file, for example `message/messages/ORU-R01.json`.
    pub file: String,
    /// The definition.
    pub definition: MessageDefinition,
}

/// The loaded v2 definitions.
#[derive(Debug)]
pub struct Corpus {
    commit: String,
    bases: BTreeMap<String, Sourced>,
    segments: BTreeMap<String, Sourced>,
    structures: BTreeMap<String, Sourced>,
    data_types: BTreeMap<String, Sourced>,
    messages: BTreeMap<String, SourcedMessage>,
}

impl Corpus {
    /// Opens the fetched tree at `root` (the directory holding `PROVENANCE.md`
    /// and `input/`).
    ///
    /// # Errors
    ///
    /// Returns [`LoadError::Fetched`] when the definitions are not on disk, the
    /// provenance names no commit, or a file fails to read or parse, and
    /// [`LoadError::DuplicateCanonical`] when two files define one URL.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, LoadError> {
        let Fetched { tree, commit } = DEFINITIONS.open(root.as_ref())?;

        let mut bases = BTreeMap::new();
        for file in V2_BASES {
            insert(&mut bases, &tree, file)?;
        }
        let mut segments = BTreeMap::new();
        for file in json_files(&tree, V2_SEGMENT_DIR)? {
            insert(&mut segments, &tree, &file)?;
        }
        let mut structures = BTreeMap::new();
        for file in json_files(&tree, V2_STRUCTURE_DIR)? {
            insert(&mut structures, &tree, &file)?;
        }
        let mut data_types = BTreeMap::new();
        for dir in V2_DATA_TYPE_DIRS {
            for file in json_files(&tree, dir)? {
                insert(&mut data_types, &tree, &file)?;
            }
        }
        let mut messages: BTreeMap<String, SourcedMessage> = BTreeMap::new();
        for file in json_files(&tree, V2_MESSAGE_DIR)? {
            let path = tree.join(&file);
            let definition: MessageDefinition = fetched::parse(&path, &fetched::read(&path)?)?;
            if definition.resource_type != "StructureDefinition" {
                return Err(LoadError::NotAStructure {
                    path: file,
                    resource_type: definition.resource_type,
                });
            }
            if let Some(first) = messages.get(&definition.url) {
                return Err(LoadError::DuplicateCanonical {
                    url: definition.url,
                    first: first.file.clone(),
                    second: file,
                });
            }
            messages.insert(definition.url.clone(), SourcedMessage { file, definition });
        }
        Ok(Self {
            commit,
            bases,
            segments,
            structures,
            data_types,
            messages,
        })
    }

    /// The pinned v2ig commit the tree was fetched at.
    #[must_use]
    pub fn commit(&self) -> &str {
        &self.commit
    }

    /// The base models the segments and message structures specialize, by URL.
    #[must_use]
    pub fn bases(&self) -> &BTreeMap<String, Sourced> {
        &self.bases
    }

    /// Every segment definition, by canonical URL.
    #[must_use]
    pub fn segments(&self) -> &BTreeMap<String, Sourced> {
        &self.segments
    }

    /// Every message structure definition, by canonical URL.
    #[must_use]
    pub fn structures(&self) -> &BTreeMap<String, Sourced> {
        &self.structures
    }

    /// Every primitive and complex data type definition, by canonical URL.
    #[must_use]
    pub fn data_types(&self) -> &BTreeMap<String, Sourced> {
        &self.data_types
    }

    /// Every message definition, by canonical URL.
    #[must_use]
    pub fn messages(&self) -> &BTreeMap<String, SourcedMessage> {
        &self.messages
    }
}

fn insert(map: &mut BTreeMap<String, Sourced>, tree: &Path, file: &str) -> Result<(), LoadError> {
    let path = tree.join(file);
    let definition: StructureDefinition = fetched::parse(&path, &fetched::read(&path)?)?;
    if definition.resource_type != "StructureDefinition" {
        return Err(LoadError::NotAStructure {
            path: file.to_owned(),
            resource_type: definition.resource_type,
        });
    }
    if let Some(first) = map.get(&definition.url) {
        return Err(LoadError::DuplicateCanonical {
            url: definition.url,
            first: first.file.clone(),
            second: file.to_owned(),
        });
    }
    map.insert(
        definition.url.clone(),
        Sourced {
            file: file.to_owned(),
            definition,
        },
    );
    Ok(())
}

/// The `.json` files directly under `tree/dir`, as sorted `dir/name` paths.
fn json_files(tree: &Path, dir: &str) -> Result<Vec<String>, LoadError> {
    let path = tree.join(dir);
    let mut files = Vec::new();
    for entry in fetched::entries(&path)? {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_json = entry.path().extension().is_some_and(|ext| ext == "json");
        if is_json && entry.path().is_file() {
            files.push(format!("{dir}/{name}"));
        }
    }
    files.sort();
    Ok(files)
}
