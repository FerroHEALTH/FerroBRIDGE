// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The element ledger of the FHIR to OMOP path: every element of every input
//! resource ends in exactly one of four outcomes.
//!
//! - **mapped:** a FHIRconnect mapping read the element into an openEHR node
//!   and an OMOCL column reads that node, named as `table.column`;
//! - **dropped:** a mapping declared that it does not run for the element
//!   (`fhirconnect::engine::outcome::Loss::Skipped`);
//! - **refused:** the run refused the resource or its composition, with the
//!   reason;
//! - **unmapped:** no mapping names the element, or the node it reached is
//!   read by no OMOCL column.
//!
//! FHIRconnect v1.0.0 is silent on an input element no mapping names, so
//! `unmapped` is its own outcome rather than a kind of `dropped`, and nothing
//! an operator sends disappears without a trace. A data-type cell reads its
//! element whole, so every element below a read element shares its outcome.
//! No specification governs the ledger: our own design.

use fhirconnect::engine::outcome::Location;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::Warning;
use omocl::resolve::program::ColumnRead;
use serde::Serialize;
use serde_json::Value;

/// What happened to one input element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ElementOutcome {
    /// The element reached an openEHR node that CDM columns read.
    Mapped {
        /// The openEHR node the FHIRconnect mapping wrote.
        node: String,
        /// The CDM columns that read the node, as `table.column`.
        targets: Vec<String>,
    },
    /// A mapping declared that it does not run for the element.
    Dropped {
        /// The declared loss.
        reason: String,
    },
    /// The run refused the resource or its composition.
    Refused {
        /// Why.
        reason: String,
    },
    /// No mapping names the element, or no CDM column reads where it went.
    Unmapped {
        /// Where the element went, when it reached an openEHR node.
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

/// One input element and its outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ElementLedger {
    /// The element, as a FHIRPath with the index of every repeating element.
    pub path: String,
    /// What happened to it.
    #[serde(flatten)]
    pub outcome: ElementOutcome,
}

/// The ledger of one input resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceLedger {
    /// The resource, as `Type/id`.
    pub resource: String,
    /// Every element of the resource with a value, in document order.
    pub elements: Vec<ElementLedger>,
}

/// How far one resource travelled, as the ledger classifies it.
#[derive(Debug, Clone, Copy)]
pub enum Fate<'a> {
    /// No FHIRconnect program maps the resource.
    Unmapped(&'a str),
    /// The run refused the resource or its composition.
    Refused(&'a str),
    /// The resource reached the CDM.
    Written {
        /// The elements the FHIRconnect run read.
        reads: &'a [Location],
        /// The losses the FHIRconnect run declared.
        warnings: &'a [Warning],
        /// The nodes the OMOCL columns read.
        columns: &'a [ColumnRead],
    },
}

/// Returns the ledger of `resource` after it met `fate`.
#[must_use]
pub fn of(resource: &Value, fate: Fate<'_>) -> ResourceLedger {
    let kind = resource
        .get("resourceType")
        .and_then(Value::as_str)
        .unwrap_or("Resource");
    let id = resource.get("id").and_then(Value::as_str).unwrap_or("-");
    let mut leaves = Vec::new();
    collect(resource, kind, &mut leaves, true);
    let elements = leaves
        .into_iter()
        .map(|path| {
            let outcome = classify(&path, fate);
            ElementLedger { path, outcome }
        })
        .collect();
    ResourceLedger {
        resource: format!("{kind}/{id}"),
        elements,
    }
}

/// Appends the path of every value below `value`, rendered from `path`.
fn collect(value: &Value, path: &str, leaves: &mut Vec<String>, root: bool) {
    match value {
        Value::Object(members) => {
            for (key, member) in members {
                if root && key == "resourceType" {
                    continue;
                }
                // NOTE: R4 json.html §Primitive Types: `_name` holds the id and
                // extensions of the primitive `name`, so it is listed under it.
                let name = key.strip_prefix('_').unwrap_or(key);
                collect(member, &format!("{path}.{name}"), leaves, false);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect(item, &format!("{path}[{index}]"), leaves, false);
            }
        }
        Value::Null => {}
        Value::Bool(_) | Value::Number(_) | Value::String(_) => leaves.push(path.to_owned()),
    }
}

/// Returns the outcome of the element at `path`.
fn classify(path: &str, fate: Fate<'_>) -> ElementOutcome {
    let (reads, warnings, columns) = match fate {
        Fate::Unmapped(reason) => {
            return ElementOutcome::Unmapped {
                reason: Some(reason.to_owned()),
            };
        }
        Fate::Refused(reason) => {
            return ElementOutcome::Refused {
                reason: reason.to_owned(),
            };
        }
        Fate::Written {
            reads,
            warnings,
            columns,
        } => (reads, warnings, columns),
    };
    if let Some(node) = reads
        .iter()
        .filter(|read| read.fhir().is_some_and(|fhir| below(path, fhir)))
        .find_map(Location::openehr)
    {
        let targets: Vec<String> = columns
            .iter()
            .filter(|column| reads_node(&column.node, node))
            .map(|column| format!("{}.{}", column.table, column.column))
            .collect();
        return if targets.is_empty() {
            ElementOutcome::Unmapped {
                reason: Some(format!("it reached {node}, which no OMOCL column reads")),
            }
        } else {
            ElementOutcome::Mapped {
                node: node.to_owned(),
                targets,
            }
        };
    }
    if let Some(warning) = warnings.iter().find(|warning| {
        matches!(warning.loss(), Loss::Skipped { .. })
            && warning
                .location()
                .fhir()
                .is_some_and(|fhir| below(path, fhir))
    }) {
        return ElementOutcome::Dropped {
            reason: warning.to_string(),
        };
    }
    ElementOutcome::Unmapped { reason: None }
}

/// Returns whether the element at `path` is `ancestor` or lies below it.
///
/// A choice element is named without its type in a FHIRPath
/// (R4 fhirpath.html §Polymorphism in FHIR), so `performedDateTime` lies
/// below `performed`; a step of `ancestor` with no index matches every index.
fn below(path: &str, ancestor: &str) -> bool {
    let mut steps = path.split('.');
    for wanted in ancestor.split('.') {
        let Some(step) = steps.next() else {
            return false;
        };
        if !step_matches(step, wanted) {
            return false;
        }
    }
    true
}

/// Returns whether the rendered step `step` is the step `wanted` names.
fn step_matches(step: &str, wanted: &str) -> bool {
    let (name, index) = split_index(step);
    let (wanted_name, wanted_index) = split_index(wanted);
    let named = name == wanted_name
        || name.strip_prefix(wanted_name).is_some_and(|suffix| {
            suffix
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_uppercase())
        });
    named && (wanted_index.is_none() || index == wanted_index)
}

/// Returns a step's name and its `[index]`, when it carries one.
fn split_index(step: &str) -> (&str, Option<&str>) {
    match step.split_once('[') {
        Some((name, rest)) => (name, Some(rest.trim_end_matches(']'))),
        None => (step, None),
    }
}

/// Returns whether a column reading `column` reads what a mapping wrote at
/// `node`: the same node, or an attribute of it with no node of its own
/// (`/value`, `/time`).
///
/// A column reading an ancestor, such as a history whose origin dates a
/// row, does not read every node below it, so only attribute steps count.
/// Both paths are compared with their name predicates left out.
fn reads_node(column: &str, node: &str) -> bool {
    let column = without_names(column);
    let node = without_names(node);
    match node.strip_prefix(&column) {
        Some("") => true,
        Some(rest) => rest.starts_with('/') && !rest.contains('['),
        None => false,
    }
}

/// Returns `path` with the name predicate of every step left out, so
/// `items[at0001,'Name']` reads `items[at0001]`.
fn without_names(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut skipping = false;
    for character in path.chars() {
        match character {
            ',' => skipping = true,
            ']' => {
                skipping = false;
                out.push(character);
            }
            _ if !skipping => out.push(character),
            _ => {}
        }
    }
    out
}

/// The counts a run report carries for the ledgers of a run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct LedgerCounts {
    /// Elements that reached a CDM column.
    pub mapped: u64,
    /// Elements a mapping declared it drops.
    pub dropped: u64,
    /// Elements of a refused resource or composition.
    pub refused: u64,
    /// Elements no mapping names or no column reads.
    pub unmapped: u64,
}

impl LedgerCounts {
    /// Returns the counts over `ledgers`.
    #[must_use]
    pub fn of(ledgers: &[ResourceLedger]) -> Self {
        let mut counts = Self::default();
        for element in ledgers.iter().flat_map(|ledger| &ledger.elements) {
            let slot = match element.outcome {
                ElementOutcome::Mapped { .. } => &mut counts.mapped,
                ElementOutcome::Dropped { .. } => &mut counts.dropped,
                ElementOutcome::Refused { .. } => &mut counts.refused,
                ElementOutcome::Unmapped { .. } => &mut counts.unmapped,
            };
            *slot = slot.saturating_add(1);
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::{below, reads_node, without_names};

    #[test]
    fn an_element_lies_below_its_ancestor_and_a_choice_below_its_name() {
        assert!(below("Condition.code.coding[0].code", "Condition.code"));
        assert!(below("Procedure.performedDateTime", "Procedure.performed"));
        assert!(below(
            "Condition.code.coding[1].system",
            "Condition.code.coding[1]"
        ));
        assert!(!below(
            "Condition.code.coding[0].system",
            "Condition.code.coding[1]"
        ));
        assert!(!below("Condition.codeable", "Condition.code"));
        assert!(!below("Condition.subject", "Condition.subject.reference"));
    }

    #[test]
    fn a_column_reads_its_node_and_the_attributes_of_it_alone() {
        let column = "/content[openEHR-EHR-ACTION.procedure.v1]/description[at0001]/items[at0002]";
        assert!(reads_node(column, column));
        assert!(reads_node(column, &format!("{column}/value")));
        assert!(!reads_node(column, &format!("{column}/items[at0003]")));
        assert!(!reads_node(
            "/content[x]/data[at0001]",
            "/content[x]/data[at0001]/events[at0002]/time"
        ));
        assert_eq!(
            "/items[at0001]/value",
            without_names("/items[at0001,'Name']/value")
        );
    }
}
