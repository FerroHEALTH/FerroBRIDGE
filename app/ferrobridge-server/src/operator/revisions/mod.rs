// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping-set revisions: every state of the console's files the bridge
//! ran, kept under `[operator] mapping_dir/..revisions`.
//!
//! A revision is a JSON manifest, `manifests/<id>.json`, naming every console
//! file by its wire path and SHA-256 digest, and the bytes are kept once per
//! digest under `blobs/<sha256>`. Ids are decimal numbers that grow by one,
//! and the running revision is the newest applied one. A save, a reload and
//! an apply that leave the console files different from the running revision
//! record a new one; an import records one unapplied. The provisioned files
//! belong to the deployment and are in no revision. The `..` prefix keeps the
//! store out of every mapping walk. No specification governs the store: our
//! own design.

pub mod apply;
pub mod diff;
pub mod routes;

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use ferrobridge_operator_api::revisions::{Origin, Revision};

use crate::operator::Operator;
use crate::operator::files;
use crate::operator::files::Root;

/// The directory under `mapping_dir` the revisions are kept in.
pub const REVISIONS: &str = "..revisions";

/// The author a revision the bridge recorded by itself names.
pub const BRIDGE: &str = "-";

/// Why the revision store could not be read or written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// A file of the store, or a console file, could not be read or written.
    #[error("cannot reach {}", path.display())]
    Io {
        /// The path.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// A manifest is no revision document.
    #[error("{} is no revision manifest", path.display())]
    Manifest {
        /// The manifest's path.
        path: PathBuf,
        /// What the decoder reported.
        #[source]
        source: serde_json::Error,
    },
    /// A blob's bytes no longer have the digest it is kept under.
    #[error("the blob {digest} does not hold the bytes of its digest")]
    Corrupt {
        /// The digest the blob is kept under.
        digest: String,
    },
    /// The console files could not be listed.
    #[error("a console mapping directory could not be read")]
    Walk(#[source] crate::walk::Error),
}

/// Who records a revision, and why.
#[derive(Debug, Clone)]
pub struct Entry {
    /// What records it.
    pub origin: Origin,
    /// The subject of the operator, or [`BRIDGE`].
    pub author: String,
    /// The reason given.
    pub reason: String,
    /// Whether it runs once recorded.
    pub applied: bool,
    /// The revision an apply restored.
    pub source: Option<String>,
}

/// The revision store under one `mapping_dir`.
#[derive(Debug, Clone)]
pub struct Store {
    /// `mapping_dir/..revisions`.
    directory: PathBuf,
}

impl Store {
    /// Returns the store under `mapping_dir`.
    #[must_use]
    pub fn new(mapping_dir: &Path) -> Self {
        Self {
            directory: mapping_dir.join(REVISIONS),
        }
    }

    /// Returns the directory of the manifests.
    fn manifests(&self) -> PathBuf {
        self.directory.join("manifests")
    }

    /// Returns the file a blob of `digest` is kept in.
    fn blob_path(&self, digest: &str) -> PathBuf {
        self.directory.join("blobs").join(digest)
    }

    /// Returns every revision, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Io`] when the store cannot be listed or a
    /// manifest read, and [`StoreError::Manifest`] for a manifest that is no
    /// revision document.
    pub fn list(&self) -> Result<Vec<Revision>, StoreError> {
        let directory = self.manifests();
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(StoreError::Io {
                    path: directory,
                    source,
                });
            }
        };
        let mut numbered = BTreeMap::new();
        for entry in entries {
            let entry = entry.map_err(|source| StoreError::Io {
                path: directory.clone(),
                source,
            })?;
            let path = entry.path();
            // NOTE: no specification governs this: our own design; a staged
            // manifest, or a name no number heads, is no revision.
            let Some(number) = number_of(&path) else {
                continue;
            };
            numbered.insert(number, read_manifest(&path)?);
        }
        Ok(numbered.into_values().collect())
    }

    /// Returns the revision `id` names, or `None` when there is none.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::list`] returns for its manifest.
    pub fn get(&self, id: &str) -> Result<Option<Revision>, StoreError> {
        if id.is_empty() || !id.chars().all(|character| character.is_ascii_digit()) {
            return Ok(None);
        }
        let path = self.manifests().join(format!("{id}.json"));
        if !path.is_file() {
            return Ok(None);
        }
        read_manifest(&path).map(Some)
    }

    /// Returns the running revision: the newest applied one.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::list`] returns.
    pub fn running(&self) -> Result<Option<Revision>, StoreError> {
        Ok(self
            .list()?
            .into_iter()
            .rev()
            .find(|revision| revision.applied))
    }

    /// Returns the bytes kept under `digest`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Io`] when the blob cannot be read and
    /// [`StoreError::Corrupt`] when its bytes have another digest.
    pub fn blob(&self, digest: &str) -> Result<Vec<u8>, StoreError> {
        let path = self.blob_path(digest);
        let bytes = std::fs::read(&path).map_err(|source| StoreError::Io { path, source })?;
        if files::sha256(&bytes) != digest {
            return Err(StoreError::Corrupt {
                digest: digest.to_owned(),
            });
        }
        Ok(bytes)
    }

    /// Records `files` as a new revision under `entry`, its parent the
    /// running revision, and returns it.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Io`] when a blob or the manifest cannot be
    /// written, and what [`Self::list`] returns.
    pub fn record(
        &self,
        entry: Entry,
        files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<Revision, StoreError> {
        let existing = self.list()?;
        let next = existing
            .iter()
            .filter_map(|revision| revision.id.parse::<u64>().ok())
            .max()
            .map_or(1, |last| last.saturating_add(1));
        let parent = existing
            .iter()
            .rev()
            .find(|revision| revision.applied)
            .map(|revision| revision.id.clone());
        let mut digests = BTreeMap::new();
        for (wire, bytes) in files {
            let digest = files::sha256(bytes);
            let path = self.blob_path(&digest);
            if !path.is_file() {
                write_replacing(&path, bytes)?;
            }
            digests.insert(wire.clone(), digest);
        }
        let revision = Revision {
            id: next.to_string(),
            parent,
            author: entry.author,
            time: now(),
            reason: entry.reason,
            origin: entry.origin,
            applied: entry.applied,
            source: entry.source,
            files: digests,
        };
        let manifest =
            serde_json::to_vec_pretty(&revision).map_err(|source| StoreError::Manifest {
                path: self.manifests(),
                source,
            })?;
        write_replacing(
            &self.manifests().join(format!("{}.json", revision.id)),
            &manifest,
        )?;
        Ok(revision)
    }

    /// Returns the bytes of every file of `revision`, by wire path.
    ///
    /// # Errors
    ///
    /// Returns what [`Self::blob`] returns.
    pub fn contents(&self, revision: &Revision) -> Result<BTreeMap<String, Vec<u8>>, StoreError> {
        revision
            .files
            .iter()
            .map(|(wire, digest)| Ok((wire.clone(), self.blob(digest)?)))
            .collect()
    }
}

/// Returns the number a manifest's name carries, or `None` for any other
/// name.
fn number_of(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".json")?;
    if stem.is_empty() || !stem.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    stem.parse().ok()
}

/// Reads the manifest at `path`.
fn read_manifest(path: &Path) -> Result<Revision, StoreError> {
    let bytes = std::fs::read(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| StoreError::Manifest {
        path: path.to_path_buf(),
        source,
    })
}

/// Writes `bytes` to `path` through a synced `..`-prefixed sibling renamed
/// over it, so a reader sees the old file or the whole new one.
fn write_replacing(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let io = |source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    };
    let name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("revision");
    let staged = path.with_file_name(format!("..{name}.{}.tmp", uuid::Uuid::new_v4()));
    crate::operator::save::write_durably(&staged, bytes).map_err(io)?;
    std::fs::rename(&staged, path).map_err(io)?;
    crate::operator::save::sync_directory(path).map_err(io)
}

/// Returns the present instant, RFC 3339 UTC to the second.
fn now() -> String {
    let now = jiff::Timestamp::now();
    jiff::Timestamp::from_second(now.as_second())
        .unwrap_or(now)
        .to_string()
}

/// Returns the bytes of every console file under `roots`, by wire path.
///
/// # Errors
///
/// Returns [`StoreError::Walk`] when a console directory cannot be listed and
/// [`StoreError::Io`] when a file cannot be read.
pub fn console_files(roots: &[Root]) -> Result<BTreeMap<String, Vec<u8>>, StoreError> {
    let console = roots
        .iter()
        .filter(|root| root.owner() == files::Owner::Console)
        .cloned()
        .collect::<Vec<_>>();
    files::list(&console)
        .map_err(StoreError::Walk)?
        .into_iter()
        .map(|located| {
            let bytes = std::fs::read(located.path()).map_err(|source| StoreError::Io {
                path: located.path().to_path_buf(),
                source,
            })?;
            Ok((located.wire().to_owned(), bytes))
        })
        .collect()
}

impl Operator {
    /// Records the console files on disk as a new revision under `entry`
    /// when they differ from the running revision, or as the baseline when
    /// none is recorded; `None` when they match it or nothing saves.
    ///
    /// # Errors
    ///
    /// Returns what [`Store::record`] and [`console_files`] return.
    pub fn settle(&self, entry: Entry) -> Result<Option<Revision>, StoreError> {
        let Some(store) = self.revisions.as_ref() else {
            return Ok(None);
        };
        let files = console_files(&self.roots)?;
        let running = store.running()?;
        let digests = files
            .iter()
            .map(|(wire, bytes)| (wire.clone(), files::sha256(bytes)))
            .collect::<BTreeMap<_, _>>();
        match running {
            Some(running) if running.files == digests => Ok(None),
            Some(_) => store.record(entry, &files).map(Some),
            None => store
                .record(
                    Entry {
                        origin: Origin::Baseline,
                        author: String::from(BRIDGE),
                        reason: String::from(
                            "the console files the bridge found before its first revision",
                        ),
                        ..entry
                    },
                    &files,
                )
                .map(Some),
        }
    }

    /// Returns the revision store, when saving is configured.
    #[must_use]
    pub const fn revisions(&self) -> Option<&Store> {
        self.revisions.as_ref()
    }
}
