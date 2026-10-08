// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one walk over a mapping directory, which every lane reads its files
//! through.
//!
//! A directory is often a Kubernetes `ConfigMap` volume, whose writer keeps the
//! files in a hidden `..<timestamp>` directory, points a `..data` symlink at
//! it, and exposes each key as a top-level file symlink through `..data`. A
//! walk that followed every symlink would read each file three times. The walk
//! therefore skips every entry whose name starts with `..`, descends only
//! into real directories, and follows a symlink only when it names a file. No
//! specification governs this: our own design.

use std::path::Path;
use std::path::PathBuf;

/// The name prefix of the entries a `ConfigMap` volume's writer keeps for itself.
const WRITER_PREFIX: &str = "..";

/// Why a directory walk stopped.
#[derive(Debug, thiserror::Error)]
#[error("cannot read {}", path.display())]
pub struct Error {
    /// The directory or entry that could not be read.
    pub path: PathBuf,
    /// What the file system reported.
    #[source]
    pub source: std::io::Error,
}

/// Returns every file under `directory` that `keep` accepts, sorted.
///
/// The walk descends into every real subdirectory, includes a symlink whose
/// target is a file, and skips a symlink whose target is a directory and every
/// entry whose name starts with `..`. The result is sorted, so two deployments
/// with the same tree load the same set in the same order.
///
/// # Errors
///
/// Returns [`Error`] naming the path when a directory cannot be listed, an
/// entry's type cannot be read, or a symlink's target cannot be resolved.
pub fn files(directory: &Path, keep: impl Fn(&Path) -> bool) -> Result<Vec<PathBuf>, Error> {
    let mut found = Vec::new();
    let mut stack = vec![directory.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current).map_err(|source| Error {
            path: current.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error {
                path: current.clone(),
                source,
            })?;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(WRITER_PREFIX))
            {
                continue;
            }
            let path = entry.path();
            // NOTE: `DirEntry::file_type` does not follow a symlink
            // (<https://doc.rust-lang.org/std/fs/struct.DirEntry.html#method.file_type>).
            let kind = entry.file_type().map_err(|source| Error {
                path: path.clone(),
                source,
            })?;
            if kind.is_dir() {
                stack.push(path);
                continue;
            }
            let target = if kind.is_symlink() {
                std::fs::metadata(&path)
            } else {
                entry.metadata()
            };
            let is_file = target
                .map_err(|source| Error {
                    path: path.clone(),
                    source,
                })?
                .is_file();
            if is_file && keep(&path) {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// Returns whether `path` carries one of the `wanted` extensions.
#[must_use]
pub fn has_extension(path: &Path, wanted: &[&str]) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| wanted.contains(&extension))
}
