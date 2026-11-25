// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping files of one load, held in memory: each file's bytes by the
//! path its diagnostics name.
//!
//! The load path compiles from a tree and never reads the disk itself. A
//! configured directory is one producer ([`Tree::read`]); the running
//! revision of the state store is the other, keyed by wire path
//! (`crate::mappings::revisions`). No specification governs this: our own
//! design.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::diagnostic::DiagnosticCode;

/// The files of one load, by path.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Tree {
    /// Each file's bytes, by the path its diagnostics name.
    files: BTreeMap<PathBuf, Arc<[u8]>>,
}

impl fmt::Debug for Tree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tree")
            .field("files", &self.files.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Tree {
    /// Reads every one of `paths` into a tree keyed by those paths.
    ///
    /// # Errors
    ///
    /// Returns the [`crate::walk::Error`] of the first file that cannot be
    /// read.
    pub fn read(paths: impl IntoIterator<Item = PathBuf>) -> Result<Self, crate::walk::Error> {
        let mut tree = Self::default();
        for path in paths {
            let bytes = std::fs::read(&path).map_err(|source| crate::walk::Error {
                path: path.clone(),
                source,
            })?;
            tree.files.insert(path, Arc::from(bytes));
        }
        Ok(tree)
    }

    /// Adds `bytes` as the file at `path`, replacing any file there.
    pub fn insert(&mut self, path: impl Into<PathBuf>, bytes: Arc<[u8]>) {
        self.files.insert(path.into(), bytes);
    }

    /// Returns the bytes of the file at `path`.
    #[must_use]
    pub fn get(&self, path: &Path) -> Option<&Arc<[u8]>> {
        self.files.get(path)
    }

    /// Returns every path, in order.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.keys().map(PathBuf::as_path)
    }

    /// Returns every file, in path order.
    pub fn iter(&self) -> impl Iterator<Item = (&Path, &Arc<[u8]>)> {
        self.files
            .iter()
            .map(|(path, bytes)| (path.as_path(), bytes))
    }

    /// Returns how many files the tree holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether the tree holds no file.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Returns the text of every file, and an `unreadable` diagnostic for
    /// each file that is not UTF-8, as the loader reports a file it cannot
    /// read.
    #[must_use]
    pub fn texts(&self) -> (Vec<(PathBuf, &str)>, Vec<Diagnostic>) {
        let mut texts = Vec::with_capacity(self.files.len());
        let mut unreadable = Vec::new();
        for (path, bytes) in &self.files {
            match std::str::from_utf8(bytes) {
                Ok(text) => texts.push((path.clone(), text)),
                Err(error) => unreadable.push(Diagnostic::error(
                    path,
                    DiagnosticCode::Unreadable,
                    error.to_string(),
                )),
            }
        }
        (texts, unreadable)
    }
}

/// Loads `tree` with `load`, which takes each file's path and text, so a
/// file that is not UTF-8 refuses the set beside every other diagnostic.
///
/// # Errors
///
/// Returns every diagnostic of the files that are not UTF-8, then every one
/// `load` raised.
pub fn load_texts<S>(
    tree: &Tree,
    load: impl FnOnce(Vec<(PathBuf, &str)>) -> Result<S, Vec<Diagnostic>>,
) -> Result<S, Vec<Diagnostic>> {
    let (texts, mut unreadable) = tree.texts();
    match load(texts) {
        Ok(set) if unreadable.is_empty() => Ok(set),
        Ok(_) => Err(unreadable),
        Err(mut diagnostics) => {
            unreadable.append(&mut diagnostics);
            Err(unreadable)
        }
    }
}

/// Loads and validates the FHIRconnect files of `tree` as one set.
///
/// # Errors
///
/// Returns what [`load_texts`] returns over the FHIRconnect loader.
pub fn fhirconnect_set(
    tree: &Tree,
) -> Result<fhirconnect::model::load::MappingSet, Vec<Diagnostic>> {
    load_texts(tree, |texts| {
        fhirconnect::model::load::load_set_from_text(texts, &fhirconnect::engine::library::Library)
    })
}

/// Loads and validates the OMOCL files of `tree` as one set.
///
/// # Errors
///
/// Returns what [`load_texts`] returns over the OMOCL loader.
pub fn omocl_set(tree: &Tree) -> Result<omocl::model::load::MappingSet, Vec<Diagnostic>> {
    load_texts(tree, |texts| {
        omocl::model::load::load_set_from_text(texts, &omocl::model::semantic::FirstPartyConverters)
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use openehr_mapping_core::diagnostic::DiagnosticCode;

    use super::{Tree, fhirconnect_set};

    #[test]
    fn a_file_that_is_not_utf8_is_an_unreadable_diagnostic_on_its_path() {
        let mut tree = Tree::default();
        tree.insert("fhirconnect/a.yml", Arc::from(&b"\xff\xfe"[..]));
        let diagnostics = fhirconnect_set(&tree).expect_err("the file is no text");
        assert_eq!(Path::new("fhirconnect/a.yml"), diagnostics[0].file());
        assert_eq!(DiagnosticCode::Unreadable, *diagnostics[0].code());
    }

    #[test]
    fn the_debug_form_names_paths_and_never_bytes() {
        let mut tree = Tree::default();
        tree.insert("fhirconnect/a.yml", Arc::from(&b"secret: 1"[..]));
        let rendered = format!("{tree:?}");
        assert!(rendered.contains("fhirconnect/a.yml"), "{rendered}");
        assert!(!rendered.contains("secret"), "{rendered}");
    }
}
