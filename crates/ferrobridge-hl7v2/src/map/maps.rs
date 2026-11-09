// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The v2-to-FHIR `ConceptMaps` the interpreter runs, read once from the
//! generated statics into rows.
//!
//! The package `hl7.fhir.uv.v2mappings` ships four kinds of `ConceptMap`, named
//! by the first word of their id: `message-` (a message structure to a
//! Bundle), `segment-` (a segment to one resource), `datatype-` (a data type
//! to a FHIR type) and `table-` (a v2 table to a code system, which a
//! terminology server answers through `$translate`). `fhir-codegen` emits
//! every row of the first three, and of a table map only its id, url and
//! groups, into `map::generated`; each row's condition, target notation and
//! `TypeInfo` texts (`StructureDefinition-TypeInfo.json`: `type`,
//! `assignment`, `mappedVia`) are parsed once here, so a form the
//! interpreter cannot run is known before a message arrives and is counted
//! when a row reaches it.
//!
//! FerroBRIDGE's corrections are generated beside the guide's maps: its
//! structure overrides and the table maps of its value package, which a
//! terminology server loads beside the guide's. [`Maps::corrected`] loads
//! them over the guide, a map with the id or the canonical url of a loaded
//! one replacing it whole and any other added; [`Maps::guide`] is the guide
//! alone. No specification governs this: our own design, under the guide's
//! leave to add a mapping locally (`mapping_guidelines.md` §General
//! Format/Approach).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use crate::map::condition::{self, AssignmentError, ConditionError, Expr, Part};
use crate::map::generated;
use crate::map::generated::model::{Declared, DeclaredRow, DeclaredTable};
use crate::map::notation::{self, NotationError, Target};

/// The kind of a `ConceptMap`, from the first word of its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// `message-`.
    Message,
    /// `segment-`.
    Segment,
    /// `datatype-`.
    Datatype,
    /// `table-`.
    Table,
    /// Any other id.
    Other,
}

impl Kind {
    fn of(id: &str) -> Self {
        match id.split_once('-').map(|(head, _)| head) {
            Some("message") => Self::Message,
            Some("segment") => Self::Segment,
            Some("datatype") => Self::Datatype,
            Some("table") => Self::Table,
            _ => Self::Other,
        }
    }
}

/// What gates a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    /// No condition: the row always applies.
    Always,
    /// A `Computable-ANTLR` condition.
    Computable(Expr),
    /// A `Narrative-Condition`, which no machine evaluates.
    Narrative,
    /// A `Computable-ANTLR` condition whose check names no operand
    /// ([`condition::ConditionError::NoOperand`]), which no machine can
    /// evaluate.
    Defective {
        /// The condition as written.
        text: String,
    },
    /// A condition in a form the interpreter does not evaluate.
    Unsupported {
        /// The property the form came under.
        property: String,
        /// The condition as written.
        text: String,
    },
}

/// A row's `assignment`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assignment {
    /// A double-quoted literal, written as given.
    Literal(String),
    /// Literals and operands joined by `+` ([`condition::assignment`]), whose
    /// texts are written joined.
    Concat(Vec<Part>),
    /// Parts with no `+` between two of them
    /// ([`condition::AssignmentError::MissingOperator`]).
    Defective(String),
    /// Any other form: an expression or a narrative instruction.
    Unsupported(String),
}

/// A row's `mappedVia`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappedVia {
    /// The id of a map, read from `ConceptMap/<id>`: a table map, or the
    /// data type map the row runs (`StructureDefinition-TypeInfo.json`,
    /// `mappedVia`: "Url of the mapping artifact for the item").
    Table(String),
    /// Any other value, such as `unspecified_mapping`.
    Unresolved(String),
}

/// One row: one source element to one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The source code as written, for example `PID-5` or `CX.1`.
    pub source: String,
    /// The source's v2 data type from its `TypeInfo`.
    pub source_type: Option<String>,
    /// The target code as written.
    pub target_code: String,
    /// The parsed target.
    pub target: Result<Target, NotationError>,
    /// The target's FHIR type from its `TypeInfo`.
    pub target_type: Option<String>,
    /// The target's `assignment`.
    pub assignment: Option<Assignment>,
    /// The target's `mappedVia`.
    pub mapped_via: Option<MappedVia>,
    /// The condition gating the row.
    pub condition: Condition,
}

/// One group of a table map: the source and target code systems a
/// translation through it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableGroup {
    /// `group.source`.
    pub source: String,
    /// `group.target`.
    pub target: String,
}

/// Where a loaded `ConceptMap` came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The guide's package.
    Guide,
    /// A FerroBRIDGE correction loaded over it.
    Supplement {
        /// Whether it replaced a map of the guide, by id or by url.
        overrides: bool,
    },
}

/// One `ConceptMap`, read into rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    /// The id.
    pub id: String,
    /// The canonical url.
    pub url: String,
    /// The title, when the map carries one.
    pub title: Option<String>,
    /// The kind, from the id.
    pub kind: Kind,
    /// The rows, in map order; empty for a table map.
    pub rows: Vec<Row>,
    /// The groups of a table map.
    pub groups: Vec<TableGroup>,
    /// Where the map came from.
    pub origin: Origin,
}

impl Map {
    /// Returns whether `other` is an alternative to this map for one value:
    /// both map one source component, with no condition, into the same child
    /// of the target, or one of them into the target itself (`$value`).
    ///
    /// The four `datatype-ei-<qualifier>-to-identifier` maps each write
    /// `EI.1` into `value` and are alternatives; maps that write distinct
    /// children, or one child only under a condition, fill parts of one
    /// target. No specification governs this: our own design, since no row
    /// of the guide names its data type map.
    #[must_use]
    pub fn alternative_to(&self, other: &Self) -> bool {
        self.unconditional_pair(other, |mine, theirs| {
            mine.source == theirs.source
                && match (reach(mine), reach(theirs)) {
                    (Some(Reach::Whole), Some(_)) | (Some(_), Some(Reach::Whole)) => true,
                    (Some(Reach::Child(a)), Some(Reach::Child(b))) => a == b,
                    _ => false,
                }
        })
    }

    /// Returns whether some row of this map and some row of `other`, both
    /// with no condition, satisfy `pair`.
    fn unconditional_pair(&self, other: &Self, pair: impl Fn(&Row, &Row) -> bool) -> bool {
        let unconditional = |row: &&Row| row.condition == Condition::Always;
        self.rows.iter().filter(unconditional).any(|mine| {
            other
                .rows
                .iter()
                .filter(unconditional)
                .any(|them| pair(mine, them))
        })
    }
}

impl Map {
    /// Returns whether `other` is the other half of one value to this map:
    /// both map a source component, with no condition, into the same child
    /// of the target or into the target itself (`$value`), each from a
    /// component the other maps nothing from.
    ///
    /// The two `datatype-eip-<half>-to-identifier` maps each write one
    /// component of an `EIP` into `value`, `EIP.1` the placer's and `EIP.2`
    /// the filler's (their titles name the halves), so each fills a whole
    /// `Identifier` of its own. No specification governs this: our own
    /// design, since no row of the guide names its data type map.
    #[must_use]
    pub fn half_of(&self, other: &Self) -> bool {
        let sources = |map: &Self| -> BTreeSet<String> {
            map.rows.iter().map(|row| row.source.clone()).collect()
        };
        let (mine, theirs) = (sources(self), sources(other));
        if !mine.is_disjoint(&theirs) {
            return false;
        }
        self.unconditional_pair(other, |row, them| match (reach(row), reach(them)) {
            (Some(Reach::Whole), Some(Reach::Whole)) => true,
            (Some(Reach::Child(a)), Some(Reach::Child(b))) => a == b,
            _ => false,
        })
    }

    /// What the map maps from and to: each group's source and target code
    /// system for a table map, else what its id names before and after
    /// `-to-`: the message type and trigger of a message map
    /// (`message-adt-a01-to-bundle` gives `adt-a01` and `bundle`), and the
    /// segment or data type of any other, without its qualifier
    /// (`datatype-ei-placer-to-identifier` gives `ei` and `identifier`).
    ///
    /// No specification governs the reading of an id: our own design, the
    /// naming [`Maps::find`] relies on, since the guide's maps other than
    /// the table maps carry no `group.source` or `group.target`.
    #[must_use]
    pub fn ends(&self) -> Vec<(String, String)> {
        if self.kind == Kind::Table {
            return self
                .groups
                .iter()
                .map(|group| (group.source.clone(), group.target.clone()))
                .collect();
        }
        let named = self
            .id
            .split_once('-')
            .and_then(|(_, rest)| rest.split_once("-to-"));
        match named {
            Some((source, target)) => {
                let source = match self.kind {
                    Kind::Message => source,
                    _ => source.split('-').next().unwrap_or(source),
                };
                vec![(source.to_owned(), target.to_owned())]
            }
            None => Vec::new(),
        }
    }

    /// The component positions the rows of this data type map read, `EIP.1`
    /// giving 1.
    #[must_use]
    pub fn components(&self) -> BTreeSet<usize> {
        self.rows
            .iter()
            .filter_map(|row| {
                // NOTE: no specification governs this: our own design; a row naming no numbered
                // component reads none, so it adds no position.
                let (_, path) = row.source.split_once('.')?;
                path.split('.').next()?.parse::<usize>().ok()
            })
            .collect()
    }
}

/// The part of its target a row writes.
enum Reach<'r> {
    /// `$value`: the target itself.
    Whole,
    /// The child with this name.
    Child(&'r str),
}

/// Returns the part of its target `row` writes, `None` for a target that
/// does not parse.
fn reach(row: &Row) -> Option<Reach<'_>> {
    match &row.target {
        Ok(Target::Value { .. }) => Some(Reach::Whole),
        Ok(Target::Path { steps, .. }) => steps.first().map(|step| Reach::Child(&step.name)),
        Err(_) => None,
    }
}

/// The guide alone.
static GUIDE: LazyLock<Maps> = LazyLock::new(|| {
    let mut maps = Maps::default();
    for declared in generated::guide::MAPS {
        maps.insert(structure(declared), false);
    }
    for declared in &generated::table::GUIDE {
        maps.insert(table(declared), false);
    }
    maps
});

/// The guide with FerroBRIDGE's corrections over it.
static CORRECTED: LazyLock<Maps> = LazyLock::new(|| {
    let mut maps = GUIDE.clone();
    for declared in generated::overrides::MAPS {
        maps.insert(structure(declared), true);
    }
    for declared in &generated::table::VALUES {
        maps.insert(table(declared), true);
    }
    maps
});

/// The `ConceptMaps` the interpreter runs, by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Maps {
    maps: BTreeMap<String, Map>,
}

impl Maps {
    /// Returns the guide's maps with FerroBRIDGE's structure overrides and
    /// value package over them, as the face runs them.
    #[must_use]
    pub fn corrected() -> &'static Self {
        &CORRECTED
    }

    /// Returns the guide's maps alone, which pin what the guide writes with
    /// no correction of FerroBRIDGE's.
    #[must_use]
    pub fn guide() -> &'static Self {
        &GUIDE
    }

    /// Returns the map with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Map> {
        self.maps.get(id)
    }

    /// Returns every map, by id.
    pub fn maps(&self) -> impl Iterator<Item = &Map> {
        self.maps.values()
    }

    /// Returns the message map whose rows name segments of `structure`, for
    /// example `ORU_R01`.
    #[must_use]
    pub fn message_map(&self, structure: &str) -> Option<&Map> {
        let dotted = format!("{structure}.");
        let follow = format!("{structure}:");
        self.maps.values().find(|map| {
            map.kind == Kind::Message
                && !map.rows.is_empty()
                && map
                    .rows
                    .iter()
                    .all(|row| row.source.starts_with(&dotted) || row.source.starts_with(&follow))
        })
    }

    /// Returns the map of `kind` from `source` to `target`: the one whose id is
    /// `<kind>-<source>-to-<target>`, else the one qualified map
    /// `<kind>-<source>-<qualifier>-to-<target>` when there is exactly one.
    ///
    /// # Errors
    ///
    /// Returns the ids of the qualified candidates when there is none or
    /// several.
    pub fn find(&self, kind: &str, source: &str, target: &str) -> Result<&Map, Vec<String>> {
        match self.qualifying(kind, source, target).as_slice() {
            [one] => Ok(one),
            several => Err(several.iter().map(|map| map.id.clone()).collect()),
        }
    }

    /// Returns whether any map of `kind` reads from `source`, whatever its
    /// target: a map whose id is `<kind>-<source>-...`.
    #[must_use]
    pub fn maps_from(&self, kind: &str, source: &str) -> bool {
        let prefix = format!("{kind}-{}-", source.to_ascii_lowercase());
        self.maps.keys().any(|id| id.starts_with(&prefix))
    }

    /// Returns every map of `kind` from `source` to `target`: the one whose
    /// id is `<kind>-<source>-to-<target>` when there is one, else each
    /// qualified map `<kind>-<source>-<qualifier>-to-<target>`, by id.
    #[must_use]
    pub fn qualifying(&self, kind: &str, source: &str, target: &str) -> Vec<&Map> {
        let source = source.to_ascii_lowercase();
        let target = target.to_ascii_lowercase();
        let exact = format!("{kind}-{source}-to-{target}");
        if let Some(map) = self.maps.get(&exact) {
            return vec![map];
        }
        let prefix = format!("{kind}-{source}-");
        let suffix = format!("-to-{target}");
        self.maps
            .values()
            .filter(|map| map.id.starts_with(&prefix) && map.id.ends_with(&suffix))
            .collect()
    }

    /// Returns these maps with `map` loaded over them as a correction.
    #[cfg(test)]
    pub(crate) fn with(mut self, map: Map) -> Self {
        self.insert(map, true);
        self
    }

    /// Adds `map`, replacing every loaded map with its id or its url, and
    /// marks it a correction when `supplement` holds.
    fn insert(&mut self, mut map: Map, supplement: bool) {
        let replaced: Vec<String> = self
            .maps
            .iter()
            .filter(|(id, loaded)| **id == map.id || loaded.url == map.url)
            .map(|(id, _)| id.clone())
            .collect();
        if supplement {
            map.origin = Origin::Supplement {
                overrides: !replaced.is_empty(),
            };
        }
        for id in replaced {
            self.maps.remove(&id);
        }
        self.maps.insert(map.id.clone(), map);
    }
}

/// Reads one generated structure map into rows.
pub(crate) fn structure(declared: &Declared) -> Map {
    Map {
        rows: declared.rows.iter().map(row).collect(),
        ..guide(declared.id, declared.url, declared.title)
    }
}

/// Reads one generated table map header.
fn table(declared: &DeclaredTable) -> Map {
    Map {
        groups: declared
            .groups
            .iter()
            .map(|(source, target)| TableGroup {
                source: String::from(source.unwrap_or_default()),
                target: String::from(target.unwrap_or_default()),
            })
            .collect(),
        ..guide(declared.id, declared.url, declared.title)
    }
}

/// A map of the guide with its id, url and title, and no rows or groups.
fn guide(id: &str, url: &str, title: Option<&str>) -> Map {
    Map {
        id: String::from(id),
        url: String::from(url),
        title: title.map(String::from),
        kind: Kind::of(id),
        rows: Vec::new(),
        groups: Vec::new(),
        origin: Origin::Guide,
    }
}

/// Reads one generated row.
fn row(declared: &DeclaredRow) -> Row {
    let target_code = String::from(declared.target.unwrap_or_default());
    let assignment = declared.assignment.map(|text| {
        match text
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .filter(|inner| !inner.contains('"'))
        {
            Some(literal) => Assignment::Literal(String::from(literal)),
            None => match condition::assignment(text) {
                Ok(parts) => Assignment::Concat(parts),
                Err(AssignmentError::MissingOperator { .. }) => {
                    Assignment::Defective(String::from(text))
                }
                Err(AssignmentError::Lex { .. } | AssignmentError::Syntax { .. }) => {
                    Assignment::Unsupported(String::from(text))
                }
            },
        }
    });
    let mapped_via = declared
        .mapped_via
        .map(|text| match text.strip_prefix("ConceptMap/") {
            Some(id) if !id.is_empty() => MappedVia::Table(String::from(id)),
            _ => MappedVia::Unresolved(String::from(text)),
        });
    Row {
        source: String::from(declared.source),
        source_type: declared.source_type.map(String::from),
        target: notation::parse(&target_code),
        target_code,
        target_type: declared.target_type.map(String::from),
        assignment,
        mapped_via,
        condition: condition(declared.source, declared.depends_on),
    }
}

/// Reads the condition of a row from its `dependsOn` entries.
///
/// A `Narrative-Condition` beside a computable one still gates the row on a
/// judgement no machine makes, so the row is narrative. A `Computable-ANTLR`
/// and a `Computable-FHIRPath` on one row state one condition twice, and the
/// ANTLR form is the one evaluated; a `Computable-FHIRPath` alone is refused,
/// since it is FHIRPath over the v2 message, not over a FHIR resource. No
/// specification governs the precedence: our own design.
fn condition(source: &str, depends_on: &[(&str, &str)]) -> Condition {
    let property = |name: &str| {
        depends_on
            .iter()
            .find_map(|(property, value)| (*property == name).then_some(*value))
    };
    if property("Narrative-Condition").is_some() {
        return Condition::Narrative;
    }
    if let Some(text) = property("Computable-ANTLR") {
        let own = condition::source_operand(source);
        return match condition::parse(text, own.as_ref()) {
            Ok(expr) => Condition::Computable(expr),
            Err(ConditionError::NoOperand { .. }) => Condition::Defective {
                text: String::from(text),
            },
            Err(ConditionError::Lex { .. } | ConditionError::Syntax { .. }) => {
                Condition::Unsupported {
                    property: String::from("Computable-ANTLR"),
                    text: String::from(text),
                }
            }
        };
    }
    if let Some(text) = property("Computable-FHIRPath") {
        return Condition::Unsupported {
            property: String::from("Computable-FHIRPath"),
            text: String::from(text),
        };
    }
    match depends_on.first() {
        Some((property, text)) => Condition::Unsupported {
            property: String::from(*property),
            text: String::from(*text),
        },
        None => Condition::Always,
    }
}

#[cfg(test)]
mod tests {
    use super::{MappedVia, Maps, Origin};
    use crate::map::generated;

    #[test]
    fn the_corrections_load_over_the_guide_as_overrides_and_additions() {
        let guide = Maps::guide();
        let corrected = Maps::corrected();
        let mut added = 0usize;
        let ids = generated::overrides::MAPS
            .iter()
            .map(|declared| declared.id)
            .chain(generated::table::VALUES.iter().map(|declared| declared.id));
        for id in ids {
            let map = corrected.get(id).expect("the correction is loaded");
            let overrides = guide.get(id).is_some();
            assert_eq!(map.origin, Origin::Supplement { overrides }, "{id}");
            if !overrides {
                added = added.saturating_add(1);
            }
        }
        assert_eq!(
            corrected.maps().count(),
            guide.maps().count().saturating_add(added)
        );
        assert_eq!(guide.maps().count(), 263);
        assert!(guide.maps().all(|map| map.origin == Origin::Guide));
    }

    #[test]
    fn a_corrected_table_map_is_added_and_the_override_names_it() {
        let guide = Maps::guide();
        let corrected = Maps::corrected();
        let id = "table-hl70202-to-contact-point-system";
        assert_eq!(
            guide.get(id),
            corrected.get(id),
            "the guide's table map stays"
        );
        let added = corrected
            .get("table-hl70202-equipment-to-contact-point-system")
            .expect("the corrected copy");
        assert_eq!(added.origin, Origin::Supplement { overrides: false });
        assert_eq!(
            added.url,
            "https://ferrobridge.eu/fhir/v2mappings/ConceptMap/table-hl70202-equipment-to-contact-point-system"
        );
        assert_eq!(added.groups.len(), 1);
        let names = |maps: &Maps| -> Vec<MappedVia> {
            maps.get("datatype-xtn-to-contactpoint")
                .expect("the XTN map")
                .rows
                .iter()
                .filter(|row| row.source == "XTN.3" && row.target_code == "system")
                .filter_map(|row| row.mapped_via.clone())
                .collect()
        };
        assert_eq!(names(guide), vec![MappedVia::Table(String::from(id))]);
        assert_eq!(
            names(corrected),
            vec![MappedVia::Table(String::from(
                "table-hl70202-equipment-to-contact-point-system"
            ))]
        );
    }
}
