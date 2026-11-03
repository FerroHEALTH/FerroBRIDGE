// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 v2 face's `ConceptMap` directory `GET /operator/hl7v2/concept-maps`
//! answers.
//!
//! The document names the directory the face read the v2-to-FHIR guide's
//! maps from and the supplement directories loaded over it, then every map
//! the face runs: its id, canonical url and title, its kind, what it maps
//! from and to, where it came from, how many rows it holds and how many
//! messages it served since the bridge started. It carries no message
//! content. No specification governs this document: our own design.

use serde::{Deserialize, Serialize};

/// What `GET /operator/hl7v2/concept-maps` answers with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConceptMaps {
    /// The directory of the guide's `ConceptMap` files.
    pub directory: String,
    /// The supplement directories, in load order, after the supplements the
    /// bridge ships.
    pub supplements: Vec<String>,
    /// Since when the face counts the messages each map served: the instant
    /// it started, RFC 3339 UTC.
    pub counted_since: String,
    /// Every map, by id.
    pub maps: Vec<ConceptMap>,
}

/// One `ConceptMap` the face runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConceptMap {
    /// The id.
    pub id: String,
    /// The canonical url.
    pub url: String,
    /// The title, when the map carries one.
    pub title: Option<String>,
    /// The kind, from the first word of the id.
    pub kind: Kind,
    /// Where the map came from.
    pub origin: Origin,
    /// What the map maps from and to: one entry per group of a table map,
    /// one for any other map.
    pub ends: Vec<Ends>,
    /// The rows the face reads from the map; a table map's rows are
    /// answered by the terminology server and count none.
    pub rows: u64,
    /// The messages whose mapping applied the map since
    /// [`ConceptMaps::counted_since`].
    pub messages: u64,
}

/// The kind of a `ConceptMap`, from the first word of its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// `message-`: a message structure to a Bundle.
    Message,
    /// `segment-`: a segment to one resource.
    Segment,
    /// `datatype-`: a data type to a FHIR type.
    Datatype,
    /// `table-`: a v2 table to a code system.
    Table,
    /// Any other id.
    Other,
}

/// Where a `ConceptMap` came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// The guide's directory.
    Guide,
    /// A supplement that adds a map the guide lacks.
    Supplement,
    /// A supplement that replaces a map of the guide.
    Override,
}

/// What one map, or one group of a table map, maps from and to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ends {
    /// The source: a message type, segment or data type as the id names it,
    /// or a table map group's source code system.
    pub source: String,
    /// The target: a FHIR type as the id names it, or a table map group's
    /// target code system.
    pub target: String,
}

#[cfg(test)]
mod tests {
    use super::{ConceptMap, ConceptMaps, Ends, Kind, Origin};

    #[test]
    fn the_document_round_trips_through_its_json() {
        let document = ConceptMaps {
            directory: String::from("/etc/ferrobridge/v2mappings"),
            supplements: vec![String::from("/etc/ferrobridge/local")],
            counted_since: String::from("2026-11-03T09:00:00Z"),
            maps: vec![ConceptMap {
                id: String::from("segment-pid-to-patient"),
                url: String::from(
                    "http://hl7.org/fhir/uv/v2mappings/ConceptMap/segment-pid-to-patient",
                ),
                title: Some(String::from("Segment PID to Patient Map")),
                kind: Kind::Segment,
                origin: Origin::Guide,
                ends: vec![Ends {
                    source: String::from("pid"),
                    target: String::from("patient"),
                }],
                rows: 31,
                messages: 2,
            }],
        };
        let json = serde_json::to_value(&document).expect("the document serializes");
        assert_eq!(json["maps"][0]["kind"], "segment");
        assert_eq!(json["maps"][0]["origin"], "guide");
        let back: ConceptMaps = serde_json::from_value(json).expect("the document reads back");
        assert_eq!(back, document);
    }
}
