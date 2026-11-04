// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading the legacy tables and HL7 table 0354 from disk.
//!
//! The export holds one directory per HL7 v2 version under
//! [`TABLES_DIR`], each with the same JSON tables: an array of flat rows per
//! file. Every row type here refuses a member it does not name, so a shape the
//! generator has not been taught is loud; a member it does not read is named
//! and discarded. A file is reported, and a defect tolerated, by its path
//! relative to [`TABLES_DIR`] (`2.5.1/elements.json`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::v2::fetched;
use crate::v2::fetched::{FetchError, Fetched, Source};

/// The directory under the fetched tree that holds one directory per version.
pub const TABLES_DIR: &str = "src/main/resources/hl7db";

/// The fetched legacy tables.
const LEGACY_TABLES: Source = Source {
    dir: TABLES_DIR,
    holds: "legacy tables",
    script: "scripts/vendor/v2-legacy.sh",
};

/// The upstream repository of the export, as the banners name it.
pub const REPOSITORY: &str = "usnistgov/igamt-hl7Tools-service";

/// Table 0354 in the vendored `hl7.terminology` package, relative to the
/// vendor directory.
pub const TABLE_0354: &str = "hl7.terminology/package/CodeSystem-v2-0354.json";

/// The canonical URL of table 0354.
pub const TABLE_0354_URL: &str = "http://terminology.hl7.org/CodeSystem/v2-0354";

/// A failure while reading the legacy tables or table 0354.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// The fetched tree did not open, or a file in it did not read or parse.
    #[error(transparent)]
    Fetched(#[from] FetchError),
    /// A directory under [`TABLES_DIR`] is not named as a version.
    #[error("{name} under {TABLES_DIR} is not a dotted version number")]
    NotAVersion {
        /// The directory name.
        name: String,
    },
    /// The table 0354 file is not the code system it should be.
    #[error("{TABLE_0354} is not the CodeSystem {TABLE_0354_URL}")]
    NotTable0354,
    /// A table 0354 concept states its status twice, or a code twice.
    #[error("{TABLE_0354}: the code {code} is given twice or with two statuses")]
    Table0354Code {
        /// The code.
        code: String,
    },
}

/// A row of `messages.json`: one message structure of the version.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageRow {
    /// The structure code, for example `ORM_O01`.
    pub id: String,
    /// The message code, for example `ORM`.
    pub msg_type_id: String,
    /// The trigger event, for example `O01`.
    pub event_id: String,
    /// Not read.
    pub section: IgnoredAny,
}

/// A row of `interactions.json`: a structure sent under a trigger event, and
/// the structure that answers it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionRow {
    /// The trigger event, for example `I14`.
    pub event_id: String,
    /// The structure code the event sends, for example `REF_I12`.
    pub sender_msg: String,
    /// The structure code that answers it, for example `RRI_I12`.
    pub receiver_msg: String,
    /// Not read.
    pub number: IgnoredAny,
}

/// A row of `groups.json`: the root or one segment group of a structure.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRow {
    /// The row id the elements name as their parent.
    pub id: u64,
    /// The structure code, a dot, and the group name (`ORM_O01.PATIENT`).
    pub name: String,
    /// The structure the group belongs to.
    pub message_id: String,
    /// Whether the row is the structure itself.
    pub is_root: bool,
    /// Whether the children are alternatives.
    pub is_choice: bool,
    /// Not read.
    pub seq: IgnoredAny,
}

/// A row of `elements.json`: a segment or a group at a place in a parent.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementRow {
    /// The row id.
    pub id: u64,
    /// The order key among the elements of the version.
    pub position: u64,
    /// The usage code, `R` or `O`.
    pub usage: String,
    /// The minimum occurrence.
    pub min: u32,
    /// The maximum occurrence, a count or `*`.
    pub max: String,
    /// The group row the element sits in.
    pub parent_id: u64,
    /// The segment the element places, for a segment element.
    pub segment_id: Option<String>,
    /// The group row the element places, for a group element.
    pub group_id: Option<u64>,
}

/// A row of `segments.json`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentRow {
    /// The segment id, for example `ORC`.
    pub id: String,
    /// The segment name.
    pub description: String,
    /// Not read.
    pub section: IgnoredAny,
}

/// A row of `fields.json`: one field of a segment.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldRow {
    /// The segment the field belongs to.
    pub segment_id: String,
    /// The field position, as text.
    pub position: String,
    /// The usage code.
    pub usage: String,
    /// The minimum repetition, as text.
    pub min: String,
    /// The maximum repetition, a count or `*`.
    pub max: String,
    /// The data element the field carries.
    pub data_element_id: String,
    /// Not read.
    pub id: IgnoredAny,
    /// Not read.
    #[serde(rename = "minLength")]
    pub min_length: IgnoredAny,
    /// Not read.
    pub truncation: IgnoredAny,
}

/// A row of `data_elements.json`: the item a field carries.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataElementRow {
    /// The HL7 item number, for example `00215`.
    pub id: String,
    /// The item name, for example `Order Control`.
    pub description: String,
    /// The data type code, `-` for a withdrawn item.
    pub datatype_id: String,
    /// The minimum length.
    pub min_length: u32,
    /// The maximum length, as text, empty where the tables give none.
    pub max_length: String,
    /// The HL7 table the item draws its codes from.
    pub table_id: Option<String>,
    /// Not read.
    pub section: IgnoredAny,
    /// Not read.
    pub truncation: IgnoredAny,
    /// Not read.
    pub conf_length: Option<IgnoredAny>,
}

/// A row of `datatypes.json`: one data type code of the version.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataTypeRow {
    /// The data type code, for example `CE` or `CE_0051`.
    pub id: String,
    /// The data type name, for example `coded element`.
    pub description: String,
    /// Not read.
    pub primitive: IgnoredAny,
    /// Not read; absent on a code the version's chapters give no section.
    pub section: Option<IgnoredAny>,
}

/// The tables of one version.
#[derive(Debug, Clone)]
pub struct VersionTables {
    /// The version as its directory spells it, for example `2.5.1`.
    pub version: String,
    /// `messages.json`.
    pub messages: Vec<MessageRow>,
    /// `groups.json`.
    pub groups: Vec<GroupRow>,
    /// `elements.json`.
    pub elements: Vec<ElementRow>,
    /// `segments.json`.
    pub segments: Vec<SegmentRow>,
    /// `fields.json`.
    pub fields: Vec<FieldRow>,
    /// `data_elements.json`.
    pub data_elements: Vec<DataElementRow>,
    /// `datatypes.json`.
    pub data_types: Vec<DataTypeRow>,
    /// `interactions.json`.
    pub interactions: Vec<InteractionRow>,
}

impl VersionTables {
    /// The path of `table` relative to [`TABLES_DIR`], for a report.
    #[must_use]
    pub fn file(&self, table: &str) -> String {
        format!("{}/{table}.json", self.version)
    }
}

/// The fetched legacy tables, one entry per version in version order.
#[derive(Debug)]
pub struct Tables {
    commit: String,
    versions: Vec<VersionTables>,
}

impl Tables {
    /// Opens the fetched tree at `root` (the directory holding
    /// `PROVENANCE.md` and `src/`).
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::Fetched`] when the tables are not on disk, the
    /// provenance names no commit, or a file fails to read or parse, and
    /// [`SourceError::NotAVersion`] for a directory not named as a version.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, SourceError> {
        let Fetched { tree, commit } = LEGACY_TABLES.open(root.as_ref())?;
        let mut named = BTreeMap::new();
        for entry in fetched::entries(&tree)? {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let key = version_key(&name)
                .ok_or_else(|| SourceError::NotAVersion { name: name.clone() })?;
            named.insert(key, name);
        }
        let mut versions = Vec::with_capacity(named.len());
        for version in named.into_values() {
            let dir = tree.join(&version);
            versions.push(VersionTables {
                messages: parse(&dir.join("messages.json"))?,
                groups: parse(&dir.join("groups.json"))?,
                elements: parse(&dir.join("elements.json"))?,
                segments: parse(&dir.join("segments.json"))?,
                fields: parse(&dir.join("fields.json"))?,
                data_elements: parse(&dir.join("data_elements.json"))?,
                data_types: parse(&dir.join("datatypes.json"))?,
                interactions: parse(&dir.join("interactions.json"))?,
                version,
            });
        }
        Ok(Self { commit, versions })
    }

    /// The pinned commit the tree was fetched at.
    #[must_use]
    pub fn commit(&self) -> &str {
        &self.commit
    }

    /// The tables of every version, in version order.
    #[must_use]
    pub fn versions(&self) -> &[VersionTables] {
        &self.versions
    }
}

/// The numeric components of a dotted version (`2.5.1` is `[2, 5, 1]`), the
/// key the versions are ordered by; `None` for any other text.
#[must_use]
pub fn version_key(version: &str) -> Option<Vec<u32>> {
    version
        .split('.')
        .map(|part| {
            let digits = !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
            if digits { part.parse().ok() } else { None }
        })
        .collect()
}

/// What table 0354 says of one structure code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeStatus<'a> {
    /// The table has no concept with the code.
    Absent,
    /// The concept states no `status` property.
    Unstated,
    /// The concept's `status` code, for example `deprecated`.
    Stated(&'a str),
}

/// The status table 0354 gives each message structure code.
#[derive(Debug)]
pub struct Table0354 {
    statuses: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeSystem {
    resource_type: String,
    url: String,
    #[serde(default)]
    concept: Vec<Concept>,
}

#[derive(Debug, Deserialize)]
struct Concept {
    code: String,
    #[serde(default)]
    property: Vec<Property>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Property {
    code: String,
    value_code: Option<String>,
}

impl Table0354 {
    /// Reads table 0354 from the vendor directory `vendor`.
    ///
    /// # Errors
    ///
    /// Returns an I/O or JSON error naming the file, [`SourceError::NotTable0354`]
    /// when the file is another resource, and [`SourceError::Table0354Code`]
    /// when a code or its status is given twice.
    pub fn open(vendor: impl AsRef<Path>) -> Result<Self, SourceError> {
        let path = vendor.as_ref().join(TABLE_0354);
        // NOTE: https://hl7.org/fhir/R5/codesystem.html defines far more members
        // than the check reads, so this projection names only the ones it reads.
        let system: CodeSystem = parse(&path)?;
        if system.resource_type != "CodeSystem" || system.url != TABLE_0354_URL {
            return Err(SourceError::NotTable0354);
        }
        let mut statuses = BTreeMap::new();
        for concept in system.concept {
            let mut status = None;
            for property in concept.property {
                if property.code != "status" {
                    continue;
                }
                if status.is_some() {
                    return Err(SourceError::Table0354Code { code: concept.code });
                }
                status = property.value_code;
            }
            if statuses.insert(concept.code.clone(), status).is_some() {
                return Err(SourceError::Table0354Code { code: concept.code });
            }
        }
        Ok(Self { statuses })
    }

    /// Returns the status table 0354 gives `code`.
    #[must_use]
    pub fn status(&self, code: &str) -> CodeStatus<'_> {
        match self.statuses.get(code) {
            None => CodeStatus::Absent,
            Some(None) => CodeStatus::Unstated,
            Some(Some(status)) => CodeStatus::Stated(status),
        }
    }
}

fn parse<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, SourceError> {
    Ok(fetched::parse(path, &fetched::read(path)?)?)
}
