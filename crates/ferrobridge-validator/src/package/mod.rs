// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One FHIR package read offline, from a `.tgz` file or an unpacked directory.
//!
//! A package is a gzipped tarball whose `package` folder holds the
//! `package.json` manifest and the resources, with an optional `.index.json`
//! derived from them (<https://hl7.org/fhir/packages.html>, §2.1.10.1 to
//! §2.1.10.4). A directory is read as the unpacked form of the same tarball:
//! `<dir>/package/package.json` and the resources beside it. Only the files
//! directly in `package/` are read; the folders below it (examples, other)
//! carry no definitions the validator resolves.

mod archive;

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;

/// The resource types a package indexes, so the set can resolve them by
/// canonical: the definitions, and the value sets and code systems a slice
/// discriminator reads its codes from.
const INDEXED: &[&str] = &["StructureDefinition", "ValueSet", "CodeSystem"];

/// The manifest file of a package.
const MANIFEST: &str = "package.json";

/// The optional index file of a package.
const INDEX: &str = ".index.json";

/// The UTF-8 byte-order mark.
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// One package: its manifest and the definitions it carries.
#[derive(Debug, Clone)]
pub struct Package {
    name: String,
    version: String,
    dependencies: BTreeMap<String, String>,
    source: PathBuf,
    resources: Vec<Resource>,
}

/// One indexed resource of a package.
#[derive(Debug, Clone)]
pub(crate) struct Resource {
    /// The file name inside `package/`.
    pub(crate) file: String,
    /// The `resourceType`.
    pub(crate) type_name: String,
    /// The canonical URL, when the resource has one.
    pub(crate) url: Option<String>,
    /// The business version, when the resource has one.
    pub(crate) version: Option<String>,
    /// Where its content is.
    pub(crate) content: Content,
}

/// The content of a resource: held, or a file read on first use.
#[derive(Debug, Clone)]
pub(crate) enum Content {
    /// The bytes, without a byte-order mark.
    Bytes(Arc<[u8]>),
    /// A file of an unpacked package.
    File(PathBuf),
}

impl Content {
    /// Returns the bytes, without a byte-order mark.
    pub(crate) fn read(&self) -> std::io::Result<Arc<[u8]>> {
        match self {
            Self::Bytes(bytes) => Ok(Arc::clone(bytes)),
            Self::File(path) => Ok(Arc::from(strip_bom(&std::fs::read(path)?))),
        }
    }
}

/// Why a package could not be read.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    /// A file or directory could not be read.
    #[error("cannot read {path}")]
    Io {
        /// The path that failed.
        path: PathBuf,
        /// The I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The path is neither a `.tgz` file nor a directory.
    #[error("{path} is neither a .tgz package nor a directory")]
    NotAPackage {
        /// The path given.
        path: PathBuf,
    },
    /// The package holds no `package/package.json`.
    #[error("{path} holds no package/package.json")]
    NoManifest {
        /// The archive or directory.
        path: PathBuf,
    },
    /// A JSON file of the package does not parse.
    #[error("{file} of the package at {path} is not the JSON a package carries")]
    Json {
        /// The archive or directory.
        path: PathBuf,
        /// The file inside `package/`.
        file: String,
        /// The parse error.
        #[source]
        source: serde_json::Error,
    },
    /// `.index.json` names a file the package does not hold.
    #[error(".index.json of the package at {path} names {file}, which the package does not hold")]
    IndexedFileMissing {
        /// The archive or directory.
        path: PathBuf,
        /// The file the index names.
        file: String,
    },
}

/// The fields of `package.json` the set reads.
#[derive(Debug, Deserialize)]
struct Manifest {
    name: String,
    version: String,
    dependencies: Option<BTreeMap<String, String>>,
}

/// The fields of `.index.json` the set reads.
#[derive(Debug, Deserialize)]
struct Index {
    files: Vec<IndexEntry>,
}

/// One row of `.index.json`.
#[derive(Debug, Deserialize)]
struct IndexEntry {
    filename: String,
    #[serde(rename = "resourceType")]
    type_name: String,
    url: Option<String>,
    version: Option<String>,
}

/// The fields of a resource the set indexes it by.
#[derive(Debug, Deserialize)]
struct Header {
    #[serde(rename = "resourceType")]
    type_name: Option<String>,
    url: Option<String>,
    version: Option<String>,
}

/// The JSON files directly inside `package/`.
trait Files {
    /// Returns the bytes of `file`, `None` when the package holds no such file.
    fn read(&mut self, file: &str) -> Result<Option<Vec<u8>>, PackageError>;

    /// Returns the content of `file` without reading a file of a directory,
    /// `None` when the package holds no such file.
    fn content(&mut self, file: &str) -> Result<Option<Content>, PackageError>;

    /// Returns the names of every JSON file, sorted.
    fn names(&mut self) -> Result<Vec<String>, PackageError>;
}

impl Package {
    /// Reads the package at `path`: a `.tgz` file or a directory holding
    /// `package/package.json`.
    ///
    /// With a `.index.json`, the resources are indexed from it and a file of
    /// a directory is read on first use; without one, every JSON file is
    /// read for its `resourceType`, `url` and `version`.
    ///
    /// # Errors
    ///
    /// Returns [`PackageError`] when the path is no package, a file cannot be
    /// read, a manifest, index or resource does not parse, or the index names
    /// a file the package does not hold.
    pub fn read(path: &Path) -> Result<Self, PackageError> {
        let metadata = std::fs::metadata(path).map_err(|source| PackageError::Io {
            path: path.to_owned(),
            source,
        })?;
        if metadata.is_dir() {
            let mut files = Directory {
                root: path.join("package"),
            };
            Self::from_files(path, &mut files)
        } else if metadata.is_file() {
            let mut files = archive::Archive::read(path)?;
            Self::from_files(path, &mut files)
        } else {
            Err(PackageError::NotAPackage {
                path: path.to_owned(),
            })
        }
    }

    /// Reads the manifest and the indexed resources from `files`.
    fn from_files(path: &Path, files: &mut dyn Files) -> Result<Self, PackageError> {
        let manifest_bytes = files
            .read(MANIFEST)?
            .ok_or_else(|| PackageError::NoManifest {
                path: path.to_owned(),
            })?;
        let manifest: Manifest = parse(path, MANIFEST, &manifest_bytes)?;
        let resources = match files.read(INDEX)? {
            Some(bytes) => indexed(path, files, &parse::<Index>(path, INDEX, &bytes)?)?,
            None => scanned(path, files)?,
        };
        Ok(Self {
            name: manifest.name,
            version: manifest.version,
            dependencies: manifest.dependencies.unwrap_or_default(),
            source: path.to_owned(),
            resources,
        })
    }

    /// Returns the package name from its manifest.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the package version from its manifest.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the dependencies its manifest names, package name to version.
    #[must_use]
    pub const fn dependencies(&self) -> &BTreeMap<String, String> {
        &self.dependencies
    }

    /// Returns the path the package was read from.
    #[must_use]
    pub fn source(&self) -> &Path {
        &self.source
    }

    /// Returns the number of `StructureDefinition` resources the package
    /// carries.
    #[must_use]
    pub fn structure_definitions(&self) -> usize {
        self.resources
            .iter()
            .filter(|resource| resource.type_name == "StructureDefinition")
            .count()
    }

    /// Returns the indexed resources.
    pub(crate) fn resources(&self) -> &[Resource] {
        &self.resources
    }
}

/// The resources `.index.json` names.
///
/// The index holds "no independent information" (packages.html §2.1.10.4),
/// so its `resourceType`, `url` and `version` stand for the files'.
fn indexed(
    path: &Path,
    files: &mut dyn Files,
    index: &Index,
) -> Result<Vec<Resource>, PackageError> {
    let mut resources = Vec::new();
    for entry in &index.files {
        if !INDEXED.contains(&entry.type_name.as_str()) {
            continue;
        }
        let Some(content) = files.content(&entry.filename)? else {
            return Err(PackageError::IndexedFileMissing {
                path: path.to_owned(),
                file: entry.filename.clone(),
            });
        };
        resources.push(Resource {
            file: entry.filename.clone(),
            type_name: entry.type_name.clone(),
            url: entry.url.clone(),
            version: entry.version.clone(),
            content,
        });
    }
    Ok(resources)
}

/// The resources of a package without an index, each file read for its
/// header.
fn scanned(path: &Path, files: &mut dyn Files) -> Result<Vec<Resource>, PackageError> {
    let mut resources = Vec::new();
    for file in files.names()? {
        if file == MANIFEST || file == INDEX {
            continue;
        }
        let Some(bytes) = files.read(&file)? else {
            continue;
        };
        let header: Header = parse(path, &file, &bytes)?;
        let Some(type_name) = header.type_name else {
            continue;
        };
        if !INDEXED.contains(&type_name.as_str()) {
            continue;
        }
        resources.push(Resource {
            file,
            type_name,
            url: header.url,
            version: header.version,
            content: Content::Bytes(Arc::from(strip_bom(&bytes))),
        });
    }
    Ok(resources)
}

/// Strips a leading UTF-8 byte-order mark.
///
/// No specification governs this: our own design. RFC 8259 §8.1 forbids the
/// mark in JSON, and published FHIR packages carry it anyway.
pub(crate) fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(BOM).unwrap_or(bytes)
}

/// Parses `bytes`, the file `file` of the package at `path`.
fn parse<T: serde::de::DeserializeOwned>(
    path: &Path,
    file: &str,
    bytes: &[u8],
) -> Result<T, PackageError> {
    serde_json::from_slice(strip_bom(bytes)).map_err(|source| PackageError::Json {
        path: path.to_owned(),
        file: file.to_owned(),
        source,
    })
}

/// The `package/` folder of an unpacked package, read file by file.
struct Directory {
    root: PathBuf,
}

impl Files for Directory {
    fn read(&mut self, file: &str) -> Result<Option<Vec<u8>>, PackageError> {
        let path = self.root.join(file);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(PackageError::Io { path, source }),
        }
    }

    fn content(&mut self, file: &str) -> Result<Option<Content>, PackageError> {
        let path = self.root.join(file);
        match std::fs::metadata(&path) {
            Ok(metadata) if !metadata.is_dir() => Ok(Some(Content::File(path))),
            Ok(_) => Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(PackageError::Io { path, source }),
        }
    }

    fn names(&mut self) -> Result<Vec<String>, PackageError> {
        let io = |source| PackageError::Io {
            path: self.root.clone(),
            source,
        };
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.root).map_err(io)? {
            let entry = entry.map_err(io)?;
            if entry.file_type().map_err(io)?.is_dir() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str()
                && Path::new(name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            {
                names.push(name.to_owned());
            }
        }
        names.sort();
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::strip_bom;

    #[test]
    fn a_leading_byte_order_mark_is_stripped() {
        assert_eq!(strip_bom(b"\xEF\xBB\xBF{}"), b"{}");
        assert_eq!(strip_bom(b"{}"), b"{}");
    }
}
