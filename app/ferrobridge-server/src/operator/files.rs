// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping files the operator API serves: the configured directories,
//! each under a root name, and the confinement of every path to one of them.
//!
//! A path on the wire is `<root>/<relative path>`, the root `fhirconnect` or
//! `omocl`. A relative path is plain names only, never `..`, `.`, a
//! `..`-prefixed name or an absolute form, and the file it names must resolve
//! inside its root once every symlink is followed, so a link that points out
//! of the tree serves nothing. No specification governs this: our own design.

use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;

/// The language of the files under one root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// FHIRconnect model, extension, context and search files.
    Fhirconnect,
    /// OMOCL files.
    Omocl,
}

impl Language {
    /// Returns the root name a wire path opens with.
    #[must_use]
    pub const fn root(self) -> &'static str {
        match self {
            Self::Fhirconnect => "fhirconnect",
            Self::Omocl => "omocl",
        }
    }
}

/// Who owns a file, and so who may change it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Owner {
    /// The deployment provisioned it; the operator API never writes it.
    Provisioned,
    /// The console saved it under `[operator] mapping_dir`.
    Console,
}

/// One configured directory and the root name a wire path reaches it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    /// The language of its files.
    language: Language,
    /// The directory.
    directory: PathBuf,
    /// Who owns its files.
    owner: Owner,
}

impl Root {
    /// Returns the provisioned root over `directory` holding files of
    /// `language`.
    #[must_use]
    pub const fn new(language: Language, directory: PathBuf) -> Self {
        Self {
            language,
            directory,
            owner: Owner::Provisioned,
        }
    }

    /// Returns the console root over `directory`, the writable tree console
    /// saves of `language` go to.
    #[must_use]
    pub const fn console(language: Language, directory: PathBuf) -> Self {
        Self {
            language,
            directory,
            owner: Owner::Console,
        }
    }

    /// Returns who owns its files.
    #[must_use]
    pub const fn owner(&self) -> Owner {
        self.owner
    }

    /// Returns the language of its files.
    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// Returns the directory.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }
}

/// Why a wire path names no file the operator API serves.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PathError {
    /// The path opens with no configured root, or names nothing below it.
    #[error("the path names no configured mapping directory")]
    UnknownRoot,
    /// The path carries something other than plain names.
    #[error("the path carries a step outside the mapping directory")]
    Step,
    /// No mapping file is there, or it resolves out of its root.
    #[error("no mapping file is at the path")]
    Absent,
    /// The file system refused a read other than a missing file.
    #[error("cannot read {}", path.display())]
    Read {
        /// The path that could not be read.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
}

/// One mapping file under a root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    /// The language of its root.
    language: Language,
    /// Who owns it.
    owner: Owner,
    /// Its path on the wire, `<root>/<relative path>` with `/` separators.
    wire: String,
    /// Its path on disk.
    path: PathBuf,
}

impl Located {
    /// Returns the language of its root.
    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// Returns who owns it.
    #[must_use]
    pub const fn owner(&self) -> Owner {
        self.owner
    }

    /// Returns its path on the wire.
    #[must_use]
    pub fn wire(&self) -> &str {
        &self.wire
    }

    /// Returns its path on disk.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// The extensions a mapping file carries under either root, compared without
/// case.
const EXTENSIONS: [&str; 2] = ["yml", "yaml"];

/// Returns whether `path` carries a mapping file extension.
#[must_use]
pub fn is_mapping(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .is_some_and(|found| EXTENSIONS.contains(&found.as_str()))
}

/// Returns every mapping file under `roots`, in root order and sorted within
/// each root.
///
/// # Errors
///
/// Returns the walk's [`crate::walk::Error`] when a directory cannot be read.
pub fn list(roots: &[Root]) -> Result<Vec<Located>, crate::walk::Error> {
    let mut found: Vec<Located> = Vec::new();
    for root in roots {
        if root.owner == Owner::Console && !root.directory.exists() {
            continue;
        }
        for path in crate::walk::files(&root.directory, is_mapping)? {
            let Some(wire) = wire_path(root, &path) else {
                continue;
            };
            // NOTE: no specification governs this: our own design; one owner
            // per file, and the provisioned root, listed first, wins a clash.
            if found.iter().any(|located| located.wire == wire) {
                continue;
            }
            found.push(Located {
                language: root.language,
                owner: root.owner,
                wire,
                path,
            });
        }
    }
    Ok(found)
}

/// Returns the wire path of `path` under `root`, or `None` when `path` is not
/// under it or a component is no UTF-8 name.
#[must_use]
pub fn wire_path(root: &Root, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(&root.directory).ok()?;
    let mut wire = String::from(root.language.root());
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return None;
        };
        wire.push('/');
        wire.push_str(name.to_str()?);
    }
    Some(wire)
}

/// Returns the wire path of `path` under the first of `roots` that holds it.
#[must_use]
pub fn wire_path_in(roots: &[Root], path: &Path) -> Option<String> {
    roots.iter().find_map(|root| wire_path(root, path))
}

/// Returns the mapping file `wire` names under `roots`.
///
/// # Errors
///
/// Returns [`PathError::UnknownRoot`] when the first segment names no root,
/// [`PathError::Step`] for an empty, `.`, `..` or `..`-prefixed segment,
/// [`PathError::Absent`] when no mapping file is there or the file resolves
/// outside its root once every symlink is followed, and [`PathError::Read`]
/// when the file system refuses a read for another reason.
pub fn resolve(roots: &[Root], wire: &str) -> Result<Located, PathError> {
    let (first, relative) = relative(wire)?;
    let mut matched = false;
    for root in roots.iter().filter(|root| root.language.root() == first) {
        matched = true;
        let path = root.directory.join(&relative);
        if !is_mapping(&path) {
            return Err(PathError::Absent);
        }
        match confined(&root.directory, &path) {
            Ok(()) => {
                return Ok(Located {
                    language: root.language,
                    owner: root.owner,
                    wire: wire.to_owned(),
                    path,
                });
            }
            Err(PathError::Absent) => {}
            Err(error) => return Err(error),
        }
    }
    Err(if matched {
        PathError::Absent
    } else {
        PathError::UnknownRoot
    })
}

/// Returns the root name `wire` opens with and the relative path after it,
/// refusing every segment that is not a plain name.
///
/// # Errors
///
/// Returns [`PathError::UnknownRoot`] when `wire` names nothing below a root
/// and [`PathError::Step`] for an empty, `.`, `..` or `..`-prefixed segment.
pub fn relative(wire: &str) -> Result<(&str, PathBuf), PathError> {
    let (first, rest) = wire.split_once('/').ok_or(PathError::UnknownRoot)?;
    let mut path = PathBuf::new();
    for segment in rest.split('/') {
        // NOTE: no specification governs this: our own design; `..`-prefixed
        // names are the ConfigMap writer's own entries, which the walk skips.
        if segment.is_empty() || segment == "." || segment.starts_with("..") {
            return Err(PathError::Step);
        }
        let mut components = Path::new(segment).components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(_)), None) => path.push(segment),
            _ => return Err(PathError::Step),
        }
    }
    Ok((first, path))
}

/// Checks that `path` is a file inside `directory` once every symlink is
/// followed.
fn confined(directory: &Path, path: &Path) -> Result<(), PathError> {
    let inside = canonical(directory)?;
    let target = canonical(path)?;
    if !target.starts_with(&inside) || !target.is_file() {
        return Err(PathError::Absent);
    }
    Ok(())
}

/// Returns `path` with every symlink followed.
fn canonical(path: &Path) -> Result<PathBuf, PathError> {
    // NOTE: no specification governs this: our own design; a missing file is
    // the legitimate absence a `404` answers, every other refusal is a fault.
    path.canonicalize().map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            PathError::Absent
        } else {
            PathError::Read {
                path: path.to_path_buf(),
                source,
            }
        }
    })
}

/// Returns the lowercase hexadecimal SHA-256 digest of `bytes`.
#[must_use]
pub fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;

    sha2::Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Language, PathError, Root, resolve, sha256};

    #[test]
    fn the_digest_is_lowercase_hex_of_sha256() {
        assert_eq!(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            sha256(b"")
        );
    }

    #[test]
    fn a_step_outside_the_tree_is_refused_before_the_disk_is_read() {
        let roots = [Root::new(
            Language::Fhirconnect,
            std::path::PathBuf::from("/ferrobridge/no/such/directory"),
        )];
        for wire in [
            "fhirconnect/../x.yml",
            "fhirconnect/./x.yml",
            "fhirconnect//x.yml",
            "fhirconnect/..data/x.yml",
        ] {
            assert!(
                matches!(resolve(&roots, wire), Err(PathError::Step)),
                "{wire}"
            );
        }
        assert!(matches!(
            resolve(&roots, "omocl/x.yml"),
            Err(PathError::UnknownRoot)
        ));
        assert!(matches!(
            resolve(&roots, "fhirconnect"),
            Err(PathError::UnknownRoot)
        ));
    }
}
