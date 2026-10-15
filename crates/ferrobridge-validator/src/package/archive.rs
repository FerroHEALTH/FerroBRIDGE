// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `.tgz` form of a package: a gzipped tarball with a `package` folder
//! (<https://hl7.org/fhir/packages.html>, §2.1.10.1).

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::Path;

use flate2::read::GzDecoder;

use std::sync::Arc;

use super::Content;
use super::Files;
use super::PackageError;
use super::strip_bom;

/// The JSON files directly inside the archive's `package/` folder.
#[derive(Debug)]
pub(super) struct Archive {
    files: BTreeMap<String, Vec<u8>>,
}

impl Archive {
    /// Reads every JSON file directly inside `package/` of the tarball at
    /// `path`.
    ///
    /// A tar stream is read front to back, so the files are held in memory
    /// before the manifest and the index are consulted.
    pub(super) fn read(path: &Path) -> Result<Self, PackageError> {
        let io = |source| PackageError::Io {
            path: path.to_owned(),
            source,
        };
        let file = std::fs::File::open(path).map_err(io)?;
        let mut archive = tar::Archive::new(GzDecoder::new(file));
        let mut files = BTreeMap::new();
        for entry in archive.entries().map_err(io)? {
            let mut entry = entry.map_err(io)?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let entry_path = entry.path().map_err(io)?;
            let Some(name) = top_level_json(&entry_path) else {
                continue;
            };
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(io)?;
            files.insert(name, bytes);
        }
        Ok(Self { files })
    }
}

/// The file name of `path` when it is a JSON file directly inside `package/`.
fn top_level_json(path: &Path) -> Option<String> {
    let mut components = path
        .components()
        .filter(|component| !matches!(component, std::path::Component::CurDir));
    let folder = components.next()?;
    let file = components.next()?;
    if components.next().is_some() || folder.as_os_str() != "package" {
        return None;
    }
    let name = file.as_os_str().to_str()?;
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        .then(|| name.to_owned())
}

impl Files for Archive {
    fn read(&mut self, file: &str) -> Result<Option<Vec<u8>>, PackageError> {
        Ok(self.files.remove(file))
    }

    fn content(&mut self, file: &str) -> Result<Option<Content>, PackageError> {
        Ok(self
            .files
            .remove(file)
            .map(|bytes| Content::Bytes(Arc::from(strip_bom(&bytes)))))
    }

    fn names(&mut self) -> Result<Vec<String>, PackageError> {
        Ok(self.files.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::top_level_json;

    #[test]
    fn only_json_files_directly_in_package_are_read() {
        assert_eq!(
            top_level_json(Path::new("package/StructureDefinition-a.json")),
            Some("StructureDefinition-a.json".to_owned())
        );
        assert_eq!(
            top_level_json(Path::new("./package/package.json")),
            Some("package.json".to_owned())
        );
        assert_eq!(
            top_level_json(Path::new("package/example/Patient-a.json")),
            None
        );
        assert_eq!(top_level_json(Path::new("other/package.json")), None);
        assert_eq!(top_level_json(Path::new("package/readme.md")), None);
    }
}
