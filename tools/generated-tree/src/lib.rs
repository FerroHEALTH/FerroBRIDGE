// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Writes a generated tree, formats it, reads it back and compares it with
//! the tree on disk.
//!
//! Both generators render every file into a scratch directory, format it
//! with the pinned `rustfmt`, read the formatted text back, and then either
//! replace the committed tree or, in check mode, report every file that
//! differs, is missing, or is stale. A tree is a map from a path relative to
//! a root to the file's text.
//!
//! No specification governs this: our own design.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A failure while writing, formatting or reading a tree.
#[derive(Debug, thiserror::Error)]
pub enum TreeError {
    /// A file or directory could not be read or written.
    #[error("cannot access {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: io::Error,
    },
    /// `rustfmt` did not run or rejected the output.
    #[error("rustfmt failed: {stderr}")]
    Rustfmt {
        /// What rustfmt printed.
        stderr: String,
    },
    /// Check mode found the tree out of date.
    #[error("the generated tree is out of date; {} file(s) differ: {}", .paths.len(), .paths.join(", "))]
    Drift {
        /// The files that differ, are missing, or are stale.
        paths: Vec<String>,
    },
}

/// Which files of a rendered tree `rustfmt` is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formatting<'a> {
    /// The crate root alone, from which `rustfmt` reaches every module the
    /// tree declares out of line.
    FromRoot(&'a str),
    /// Every file of the tree.
    EachFile,
}

/// Returns `files` as `rustfmt` formats them under `config`, written to and
/// read back from a scratch directory.
///
/// # Errors
///
/// Returns [`TreeError::Io`] when the scratch directory cannot be made,
/// written or read, and [`TreeError::Rustfmt`] when `rustfmt` does not run
/// or refuses the files.
pub fn formatted(
    files: &BTreeMap<String, String>,
    formatting: Formatting<'_>,
    config: &Path,
) -> Result<BTreeMap<String, String>, TreeError> {
    let scratch = tempfile::tempdir().map_err(|source| TreeError::Io {
        path: PathBuf::from("(temporary directory)"),
        source,
    })?;
    let root = scratch.path();
    write(root, files)?;
    match formatting {
        Formatting::FromRoot(crate_root) => rustfmt([root.join(crate_root)], config)?,
        Formatting::EachFile => rustfmt(files.keys().map(|relative| root.join(relative)), config)?,
    }
    read(root, files.keys())
}

/// Removes the directory `dir` and everything under it, when it exists.
///
/// # Errors
///
/// Returns [`TreeError::Io`] naming the directory that could not be removed.
pub fn clear(dir: &Path) -> Result<(), TreeError> {
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|source| TreeError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

/// Checks `files` against the tree on disk, as [`drift`] compares them.
///
/// # Errors
///
/// Returns [`TreeError::Drift`] naming every file that differs, is missing or
/// is stale, and [`TreeError::Io`] when an owned directory cannot be listed.
pub fn check(
    files: &BTreeMap<String, String>,
    path_of: impl Fn(&str) -> PathBuf,
    owned: &[(PathBuf, &str)],
) -> Result<(), TreeError> {
    let paths = drift(files, path_of, owned)?;
    if paths.is_empty() {
        Ok(())
    } else {
        Err(TreeError::Drift { paths })
    }
}

/// Writes every file of `files` under `root`, creating the directories above
/// each.
///
/// # Errors
///
/// Returns [`TreeError::Io`] naming the directory or file that could not be
/// written.
pub fn write(root: &Path, files: &BTreeMap<String, String>) -> Result<(), TreeError> {
    for (relative, content) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| TreeError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(&path, content).map_err(|source| TreeError::Io { path, source })?;
    }
    Ok(())
}

/// Reads back the files `names` names under `root`.
///
/// # Errors
///
/// Returns [`TreeError::Io`] naming the file that could not be read.
pub fn read<'a>(
    root: &Path,
    names: impl IntoIterator<Item = &'a String>,
) -> Result<BTreeMap<String, String>, TreeError> {
    let mut out = BTreeMap::new();
    for relative in names {
        let path = root.join(relative);
        let content = fs::read_to_string(&path).map_err(|source| TreeError::Io {
            path: path.clone(),
            source,
        })?;
        out.insert(relative.clone(), content);
    }
    Ok(out)
}

/// Formats `paths` in place with the `rustfmt` on `PATH`, edition 2024,
/// under `config` when that file exists.
///
/// # Errors
///
/// Returns [`TreeError::Io`] when `rustfmt` does not start and
/// [`TreeError::Rustfmt`] with what it printed when it refuses the files.
pub fn rustfmt<P: AsRef<Path>>(
    paths: impl IntoIterator<Item = P>,
    config: &Path,
) -> Result<(), TreeError> {
    let mut command = Command::new("rustfmt");
    command.arg("--edition").arg("2024");
    if config.is_file() {
        command.arg("--config-path").arg(config);
    }
    for path in paths {
        command.arg(path.as_ref());
    }
    let output = command.output().map_err(|source| TreeError::Io {
        path: PathBuf::from("rustfmt"),
        source,
    })?;
    if !output.status.success() {
        return Err(TreeError::Rustfmt {
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

/// The relative paths that differ between `files` and the tree on disk,
/// sorted.
///
/// `path_of` gives the place on disk of each relative path. A file that
/// differs is named as it is, one that is absent with ` (missing)` after it,
/// and a file under one of the `owned` directories (each with the prefix its
/// files carry in `files`) that `files` does not hold with ` (stale)`. An
/// owned directory that does not exist holds nothing stale.
///
/// # Errors
///
/// Returns [`TreeError::Io`] when an owned directory cannot be listed.
pub fn drift(
    files: &BTreeMap<String, String>,
    path_of: impl Fn(&str) -> PathBuf,
    owned: &[(PathBuf, &str)],
) -> Result<Vec<String>, TreeError> {
    let mut drift = Vec::new();
    for (relative, content) in files {
        match fs::read_to_string(path_of(relative)) {
            Ok(existing) if existing == *content => {}
            Ok(_) => drift.push(relative.clone()),
            Err(_) => drift.push(format!("{relative} (missing)")),
        }
    }
    for (dir, prefix) in owned {
        if !dir.is_dir() {
            continue;
        }
        for relative in files_under(dir, prefix)? {
            if !files.contains_key(&relative) {
                drift.push(format!("{relative} (stale)"));
            }
        }
    }
    drift.sort();
    Ok(drift)
}

/// Every file under `dir`, recursively, as `prefix/…` relative paths, or
/// relative to `dir` itself when `prefix` is empty.
///
/// # Errors
///
/// Returns [`TreeError::Io`] naming the directory that could not be listed.
pub fn files_under(dir: &Path, prefix: &str) -> Result<Vec<String>, TreeError> {
    let entries = fs::read_dir(dir).map_err(|source| TreeError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| TreeError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if entry.path().is_dir() {
            out.extend(files_under(&entry.path(), &relative)?);
        } else {
            out.push(relative);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use super::{drift, files_under, read, write};

    fn tree(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
            .collect()
    }

    #[test]
    fn a_written_tree_reads_back_as_it_was_written() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let files = tree(&[("lib.rs", "mod a;\n"), ("a/mod.rs", "pub fn f() {}\n")]);
        write(root.path(), &files).expect("the tree writes");
        assert_eq!(read(root.path(), files.keys()).expect("it reads"), files);
    }

    #[test]
    fn drift_names_a_changed_a_missing_and_a_stale_file_in_order() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let on_disk = tree(&[
            ("gen/same.rs", "same\n"),
            ("gen/changed.rs", "old\n"),
            ("gen/stale.rs", "gone\n"),
        ]);
        write(root.path(), &on_disk).expect("the tree writes");
        let rendered = tree(&[
            ("gen/same.rs", "same\n"),
            ("gen/changed.rs", "new\n"),
            ("gen/added.rs", "added\n"),
        ]);
        let owned = [(root.path().join("gen"), "gen")];
        let found = drift(&rendered, |relative| root.path().join(relative), &owned)
            .expect("the comparison runs");
        assert_eq!(
            found,
            [
                "gen/added.rs (missing)",
                "gen/changed.rs",
                "gen/stale.rs (stale)"
            ]
        );
    }

    #[test]
    fn an_owned_directory_that_does_not_exist_holds_nothing_stale() {
        let root = tempfile::tempdir().expect("a scratch directory");
        let owned = [(root.path().join("absent"), "absent")];
        let found = drift(
            &BTreeMap::new(),
            |relative| root.path().join(relative),
            &owned,
        )
        .expect("the comparison runs");
        assert_eq!(found, Vec::<String>::new());
    }

    #[test]
    fn an_empty_prefix_lists_paths_relative_to_the_directory() {
        let root = tempfile::tempdir().expect("a scratch directory");
        fs::create_dir_all(root.path().join("b")).expect("the directory");
        fs::write(root.path().join("b/c.rs"), "").expect("the file");
        assert_eq!(files_under(root.path(), "").expect("it lists"), ["b/c.rs"]);
    }

    #[test]
    fn files_under_lists_nested_files_under_their_prefix() {
        let root = tempfile::tempdir().expect("a scratch directory");
        fs::create_dir_all(root.path().join("a/b")).expect("the directories");
        fs::write(root.path().join("a/b/c.rs"), "").expect("the file");
        let mut found = files_under(&root.path().join("a"), "a").expect("it lists");
        found.sort();
        assert_eq!(found, ["a/b/c.rs"]);
    }
}
