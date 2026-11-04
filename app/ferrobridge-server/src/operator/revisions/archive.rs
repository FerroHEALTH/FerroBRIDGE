// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping-set archive: a revision's console files as one ZIP, and a
//! ZIP read back into files by wire path.
//!
//! An export writes the files in wire-path order, each deflated at one fixed
//! level with the ZIP epoch as its time and fixed permissions, so the same
//! files always give the same bytes and an import followed by an export
//! answers the archive it read. An import reads at most [`MAX_ENTRIES`]
//! entries and [`MAX_BYTES`] of content, skips directory entries, and
//! refuses an entry whose name is no console mapping file. No specification
//! governs the archive: our own design.

use std::collections::BTreeMap;
use std::io::{Cursor, Read as _, Write as _};

use crate::mappings::files;
use crate::mappings::files::Language;

/// The most entries an import reads.
pub const MAX_ENTRIES: usize = 4096;

/// The most content bytes an import reads, over every entry.
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// The deflate level of every entry an export writes.
const LEVEL: i64 = 6;

/// The permissions every entry an export writes carries.
const MODE: u32 = 0o644;

/// Why an archive could not be written or read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ArchiveError {
    /// The ZIP reader or writer refused.
    #[error("the archive is no ZIP this bridge reads or writes")]
    Zip(#[source] zip::result::ZipError),
    /// An entry's bytes could not be copied.
    #[error("an archive entry could not be read or written")]
    Io(#[source] std::io::Error),
    /// The archive holds more entries than an import reads.
    #[error("the archive holds more than {MAX_ENTRIES} entries")]
    TooManyEntries,
    /// The archive's content is larger than an import reads.
    #[error("the archive's content is larger than {MAX_BYTES} bytes")]
    TooLarge,
    /// An entry names no console mapping file.
    #[error("an archive entry names no FHIRconnect or OMOCL mapping file")]
    BadEntry,
    /// Two entries name the same file.
    #[error("two archive entries name the same file")]
    Duplicate,
}

/// Returns `contents`, by wire path, as one ZIP.
///
/// # Errors
///
/// Returns [`ArchiveError::Zip`] or [`ArchiveError::Io`] when the writer
/// refuses.
pub fn export(contents: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, ArchiveError> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(LEVEL))
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(MODE);
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (wire, bytes) in contents {
        writer
            .start_file(wire.as_str(), options)
            .map_err(ArchiveError::Zip)?;
        writer.write_all(bytes).map_err(ArchiveError::Io)?;
    }
    Ok(writer.finish().map_err(ArchiveError::Zip)?.into_inner())
}

/// Returns the files `archive` holds, by wire path.
///
/// # Errors
///
/// Returns [`ArchiveError::Zip`] for bytes that are no ZIP,
/// [`ArchiveError::TooManyEntries`] and [`ArchiveError::TooLarge`] past the
/// ceilings, [`ArchiveError::BadEntry`] for an entry that names no console
/// mapping file, and [`ArchiveError::Duplicate`] for two entries of one
/// file.
pub fn import(archive: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, ArchiveError> {
    let mut reader = zip::ZipArchive::new(Cursor::new(archive)).map_err(ArchiveError::Zip)?;
    if reader.len() > MAX_ENTRIES {
        return Err(ArchiveError::TooManyEntries);
    }
    let mut contents = BTreeMap::new();
    let mut left = MAX_BYTES;
    for index in 0..reader.len() {
        let entry = reader.by_index(index).map_err(ArchiveError::Zip)?;
        if entry.is_dir() {
            continue;
        }
        let wire = entry.name().map_err(ArchiveError::Zip)?.into_owned();
        let console =
            files::relative(&wire).is_ok_and(|(root, _relative)| Language::of_root(root).is_some());
        if !console || !files::is_mapping(std::path::Path::new(&wire)) {
            return Err(ArchiveError::BadEntry);
        }
        let mut bytes = Vec::new();
        // NOTE: no specification governs this: our own design; the ceiling
        // counts the bytes inflated, never the size an entry claims.
        let read = entry
            .take(left.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(ArchiveError::Io)?;
        let read = u64::try_from(read).unwrap_or(u64::MAX);
        if read > left {
            return Err(ArchiveError::TooLarge);
        }
        left = left.saturating_sub(read);
        if contents.insert(wire, bytes).is_some() {
            return Err(ArchiveError::Duplicate);
        }
    }
    Ok(contents)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{ArchiveError, export, import};

    fn set() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            (String::from("omocl/b.yml"), b"b: 1\n".to_vec()),
            (String::from("fhirconnect/a/c.yml"), b"c: 2\n".to_vec()),
        ])
    }

    #[test]
    fn an_export_is_byte_identical_after_an_import() {
        let first = export(&set()).expect("the set exports");
        let read = import(&first).expect("the export imports");
        assert_eq!(set(), read);
        assert_eq!(first, export(&read).expect("the import exports"));
    }

    #[test]
    fn an_entry_outside_the_console_roots_is_refused() {
        for name in [
            "etc/passwd.yml",
            "fhirconnect/../x.yml",
            "fhirconnect/x.txt",
        ] {
            let archive = export(&BTreeMap::from([(String::from(name), b"x".to_vec())]))
                .expect("the archive writes");
            assert!(
                matches!(import(&archive), Err(ArchiveError::BadEntry)),
                "{name}"
            );
        }
    }

    #[test]
    fn bytes_that_are_no_zip_are_refused() {
        assert!(matches!(import(b"no zip"), Err(ArchiveError::Zip(_))));
    }
}
