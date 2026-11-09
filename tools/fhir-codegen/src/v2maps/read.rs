// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading a directory of v2-to-FHIR `ConceptMap`s into the rows the emitter
//! renders.
//!
//! Each `ConceptMap-*.json` file is one R4 `ConceptMap`
//! (<https://hl7.org/fhir/R4/conceptmap.html>). A structure map's rows are
//! its group elements' targets in map order, each with the element's code,
//! the `TypeInfo` sub-extensions `type`, `assignment` and `mappedVia`
//! (`StructureDefinition-TypeInfo.json` of the guide) and the target's
//! `dependsOn` entries as written; a table map keeps each group's `source`
//! and `target`. Everything else is left to the crate that reads the
//! statics, so the conditions and notations are parsed in one place.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The canonical url of the guide's `TypeInfo` extension.
pub const TYPE_INFO: &str = "http://hl7.org/fhir/uv/v2mappings/StructureDefinition/TypeInfo";

/// The kind of a map, from the first word of its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// `message-`: a message structure to a Bundle.
    Message,
    /// `segment-`: a segment to one resource.
    Segment,
    /// `datatype-`: a data type to a FHIR type.
    Datatype,
    /// `table-`: a v2 table to a code system, or back.
    Table,
}

impl Kind {
    /// The kind `id` names, `None` for an id of no known kind.
    #[must_use]
    pub fn of(id: &str) -> Option<Self> {
        match id.split_once('-').map(|(head, _)| head) {
            Some("message") => Some(Self::Message),
            Some("segment") => Some(Self::Segment),
            Some("datatype") => Some(Self::Datatype),
            Some("table") => Some(Self::Table),
            _ => None,
        }
    }
}

/// One row of a structure map: one target of one group element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// `element.code`.
    pub source: String,
    /// The element's `TypeInfo` `type`.
    pub source_type: Option<String>,
    /// `target.code`.
    pub target: Option<String>,
    /// The target's `TypeInfo` `type`.
    pub target_type: Option<String>,
    /// The target's `TypeInfo` `assignment`.
    pub assignment: Option<String>,
    /// The target's `TypeInfo` `mappedVia`.
    pub mapped_via: Option<String>,
    /// `target.dependsOn`, each `property` and `value`, in order.
    pub depends_on: Vec<(String, String)>,
}

/// One group of a table map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// `group.source`.
    pub source: Option<String>,
    /// `group.target`.
    pub target: Option<String>,
}

/// One `ConceptMap` as the emitter renders it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    /// The id.
    pub id: String,
    /// The canonical url.
    pub url: String,
    /// The title.
    pub title: Option<String>,
    /// The description.
    pub description: Option<String>,
    /// The kind, from the id.
    pub kind: Kind,
    /// The rows of a structure map, in map order; empty for a table map.
    pub rows: Vec<Row>,
    /// The groups of a table map; empty for a structure map.
    pub groups: Vec<Group>,
}

/// A refusal to read a directory of maps.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// A directory or file could not be read.
    #[error("{path} could not be read")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A file is no JSON.
    #[error("{path} is no JSON document")]
    Json {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
    /// A file is no `ConceptMap` of the shape the emitter reads.
    #[error("{path}: {reason}")]
    Shape {
        /// The path.
        path: PathBuf,
        /// What is wrong, at which member.
        reason: String,
    },
    /// Two files of one directory carry one id.
    #[error("{path} repeats the id {id}")]
    Repeated {
        /// The second file.
        path: PathBuf,
        /// The id.
        id: String,
    },
}

/// Reads every `ConceptMap-*.json` file of `directory`, by id.
///
/// # Errors
///
/// Returns [`ReadError`] when the directory or a file cannot be read, a file
/// is not a `ConceptMap` with an id of a known kind, a url and the members
/// the emitter reads in their R4 types, its file name is not
/// `ConceptMap-<id>.json`, or two files carry one id.
pub fn directory(directory: &Path) -> Result<BTreeMap<String, Map>, ReadError> {
    let io = |source| ReadError::Io {
        path: directory.to_path_buf(),
        source,
    };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(directory)
        .map_err(io)?
        .map(|entry| entry.map(|entry| entry.path()).map_err(io))
        .collect::<Result<_, _>>()?;
    paths.retain(|path| {
        path.file_name()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|name| {
                name.starts_with("ConceptMap-")
                    && Path::new(name)
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            })
    });
    paths.sort();
    let mut maps = BTreeMap::new();
    for path in paths {
        let text = std::fs::read_to_string(&path).map_err(|source| ReadError::Io {
            path: path.clone(),
            source,
        })?;
        let value: Value = serde_json::from_str(&text).map_err(|source| ReadError::Json {
            path: path.clone(),
            source,
        })?;
        let map = concept_map(&value).map_err(|reason| ReadError::Shape {
            path: path.clone(),
            reason,
        })?;
        let named = path.file_name().and_then(std::ffi::OsStr::to_str);
        if named != Some(format!("ConceptMap-{}.json", map.id).as_str()) {
            return Err(ReadError::Shape {
                path,
                reason: format!("the file is not named ConceptMap-{}.json", map.id),
            });
        }
        if maps.contains_key(&map.id) {
            return Err(ReadError::Repeated { path, id: map.id });
        }
        maps.insert(map.id.clone(), map);
    }
    Ok(maps)
}

/// Reads one `ConceptMap` resource.
fn concept_map(value: &Value) -> Result<Map, String> {
    if value.get("resourceType").and_then(Value::as_str) != Some("ConceptMap") {
        return Err(String::from("resourceType is not ConceptMap"));
    }
    let id = string(value, "id")?.ok_or("no id")?;
    let url = string(value, "url")?.ok_or("no url")?;
    let kind = Kind::of(&id).ok_or_else(|| format!("the id {id} names no kind of map"))?;
    let title = string(value, "title")?;
    let description = string(value, "description")?;
    let mut rows = Vec::new();
    let mut groups = Vec::new();
    for group in array(value, "group")? {
        if kind == Kind::Table {
            groups.push(Group {
                source: string(group, "source")?,
                target: string(group, "target")?,
            });
            continue;
        }
        for element in array(group, "element")? {
            let source = string(element, "code")?.ok_or("group.element has no code")?;
            let source_type = type_info(element, "type")?;
            for target in array(element, "target")? {
                let mut depends_on = Vec::new();
                for entry in array(target, "dependsOn")? {
                    let property = string(entry, "property")?.ok_or("dependsOn has no property")?;
                    let value = string(entry, "value")?.ok_or("dependsOn has no value")?;
                    depends_on.push((property, value));
                }
                rows.push(Row {
                    source: source.clone(),
                    source_type: source_type.clone(),
                    target: string(target, "code")?,
                    target_type: type_info(target, "type")?,
                    assignment: type_info(target, "assignment")?,
                    mapped_via: type_info(target, "mappedVia")?,
                    depends_on,
                });
            }
        }
    }
    Ok(Map {
        id,
        url,
        title,
        description,
        kind,
        rows,
        groups,
    })
}

/// The string member `name` of `value`, `None` when it is absent.
fn string(value: &Value, name: &str) -> Result<Option<String>, String> {
    match value.get(name) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(format!("{name} is not a string")),
    }
}

/// The array member `name` of `value`, empty when it is absent.
fn array<'v>(value: &'v Value, name: &str) -> Result<&'v [Value], String> {
    match value.get(name) {
        None => Ok(&[]),
        Some(Value::Array(items)) => Ok(items),
        Some(_) => Err(format!("{name} is not an array")),
    }
}

/// The `TypeInfo` sub-extension `name` of `value` as text: its `valueCode`,
/// `valueString`, `valueUrl` or `valueUri`.
fn type_info(value: &Value, name: &str) -> Result<Option<String>, String> {
    for extension in array(value, "extension")? {
        if extension.get("url").and_then(Value::as_str) != Some(TYPE_INFO) {
            continue;
        }
        for inner in array(extension, "extension")? {
            if inner.get("url").and_then(Value::as_str) != Some(name) {
                continue;
            }
            for member in ["valueCode", "valueString", "valueUrl", "valueUri"] {
                if let Some(text) = string(inner, member)? {
                    return Ok(Some(text));
                }
            }
            return Err(format!("TypeInfo {name} carries no text value"));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Kind, concept_map};

    #[test]
    fn a_structure_map_reads_one_row_per_target_with_its_type_info() {
        let map = concept_map(&json!({
            "resourceType": "ConceptMap",
            "id": "datatype-cx-to-identifier",
            "url": "http://hl7.org/fhir/uv/v2mappings/ConceptMap/datatype-cx-to-identifier",
            "group": [{ "element": [{
                "code": "CX.1",
                "extension": [{ "url": super::TYPE_INFO, "extension": [
                    { "url": "type", "valueCode": "ST" }
                ]}],
                "target": [
                    { "code": "value", "dependsOn": [
                        { "property": "Computable-ANTLR", "value": "CX.1 VALUED" }
                    ]},
                    { "code": "type.coding.code", "extension": [{ "url": super::TYPE_INFO,
                        "extension": [{ "url": "assignment", "valueString": "\"MR\"" }] }] }
                ]
            }]}]
        }))
        .expect("the map reads");
        assert_eq!(map.kind, Kind::Datatype);
        assert_eq!(map.rows.len(), 2);
        assert_eq!(map.rows[0].source_type.as_deref(), Some("ST"));
        assert_eq!(
            map.rows[0].depends_on,
            vec![(
                String::from("Computable-ANTLR"),
                String::from("CX.1 VALUED")
            )]
        );
        assert_eq!(map.rows[1].assignment.as_deref(), Some("\"MR\""));
    }

    #[test]
    fn a_map_of_no_known_kind_is_refused() {
        let refused = concept_map(&json!({
            "resourceType": "ConceptMap", "id": "other-a-to-b", "url": "http://example.org/a"
        }));
        assert!(refused.is_err_and(|reason| reason.contains("names no kind")));
    }

    #[test]
    fn a_type_info_member_of_another_type_is_refused() {
        let refused = concept_map(&json!({
            "resourceType": "ConceptMap", "id": "segment-a-to-b", "url": "http://example.org/a",
            "group": [{ "element": [{ "code": "A-1", "target": [{ "code": "b",
                "extension": [{ "url": super::TYPE_INFO,
                    "extension": [{ "url": "type", "valueInteger": 1 }] }] }] }] }]
        }));
        assert!(refused.is_err_and(|reason| reason.contains("no text value")));
    }
}
