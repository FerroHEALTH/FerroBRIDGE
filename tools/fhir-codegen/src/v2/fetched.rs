// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The fetched trees the v2 root set reads, each checked out at the commit
//! its `PROVENANCE.md` pins: the HL7 v2 definitions and the legacy tables.
//!
//! Both are fetched at build time into an ignored directory, so a tree that
//! is not on disk, a provenance with no pin, and a file that cannot be read
//! or parsed are refused alike, naming the path. No specification governs
//! this: our own design.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A failure while opening a fetched tree or reading a file in it.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// The fetched tree is not on disk.
    #[error("{path} holds no HL7 v2 {holds}; run {script} to fetch them at the pin")]
    NotFetched {
        /// The directory that was expected.
        path: PathBuf,
        /// What the tree holds.
        holds: &'static str,
        /// The script that fetches it.
        script: &'static str,
    },
    /// The provenance file names no pinned commit.
    #[error("{path} names no pinned commit (a line \"- Pin: commit `<sha>`\")")]
    NoPin {
        /// The provenance file.
        path: PathBuf,
    },
    /// A directory could not be listed or a file could not be read.
    #[error("cannot read {path}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// A file is not the JSON the projection expects.
    #[error("cannot parse {path}")]
    Json {
        /// The file that failed to parse.
        path: PathBuf,
        /// The underlying JSON error.
        #[source]
        source: serde_json::Error,
    },
}

/// A fetched tree: where its content is, and what it holds.
#[derive(Debug, Clone, Copy)]
pub struct Source {
    /// The directory under the fetched root that holds the content.
    pub dir: &'static str,
    /// What the tree holds, as a refusal names it.
    pub holds: &'static str,
    /// The script that fetches it.
    pub script: &'static str,
}

/// An opened fetched tree.
#[derive(Debug, Clone)]
pub struct Fetched {
    /// The content directory.
    pub tree: PathBuf,
    /// The commit the provenance pins.
    pub commit: String,
}

impl Source {
    /// Opens the tree under `root`, the directory holding `PROVENANCE.md`.
    ///
    /// # Errors
    ///
    /// Returns [`FetchError::NotFetched`] when the content directory is not on
    /// disk, [`FetchError::NoPin`] when the provenance names no commit, and
    /// [`FetchError::Io`] when the provenance cannot be read.
    pub fn open(&self, root: &Path) -> Result<Fetched, FetchError> {
        let tree = root.join(self.dir);
        if !tree.is_dir() {
            return Err(FetchError::NotFetched {
                path: tree,
                holds: self.holds,
                script: self.script,
            });
        }
        let provenance = root.join("PROVENANCE.md");
        let commit =
            pinned_commit(&read(&provenance)?).ok_or(FetchError::NoPin { path: provenance })?;
        Ok(Fetched { tree, commit })
    }
}

/// Returns the entries of the directory `dir`.
///
/// # Errors
///
/// Returns [`FetchError::Io`] naming `dir` when it cannot be listed.
pub fn entries(dir: &Path) -> Result<Vec<fs::DirEntry>, FetchError> {
    let io = |source| FetchError::Io {
        path: dir.to_path_buf(),
        source,
    };
    fs::read_dir(dir)
        .map_err(io)?
        .map(|entry| entry.map_err(io))
        .collect()
}

/// Returns the text of the file `path`.
///
/// # Errors
///
/// Returns [`FetchError::Io`] naming `path` when it cannot be read.
pub fn read(path: &Path) -> Result<String, FetchError> {
    fs::read_to_string(path).map_err(|source| FetchError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Parses `text`, read from `path`, as JSON.
///
/// # Errors
///
/// Returns [`FetchError::Json`] naming `path` when the text is not the JSON
/// `T` expects.
pub fn parse<T: for<'de> serde::Deserialize<'de>>(
    path: &Path,
    text: &str,
) -> Result<T, FetchError> {
    serde_json::from_str(text).map_err(|source| FetchError::Json {
        path: path.to_path_buf(),
        source,
    })
}

/// The commit a `PROVENANCE.md` pins, from its line that opens `- Pin: commit`.
fn pinned_commit(provenance: &str) -> Option<String> {
    provenance.lines().find_map(|line| {
        let rest = line.strip_prefix("- Pin: commit `")?;
        let (sha, _) = rest.split_once('`')?;
        let hex = sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit());
        hex.then(|| sha.to_owned())
    })
}
