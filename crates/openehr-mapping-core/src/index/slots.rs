// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The archetype slots a template leaves open.
//!
//! The `openehr-sdt` builder records, on the node that owns each constrained
//! attribute, the `ARCHETYPE_SLOT`s the template keeps open there
//! (`WebTemplateNode::closed_attributes`), each with its include and exclude
//! patterns. The index keys them by the attribute's absolute path, so a caller
//! can tell an archetype the template could hold at a place but holds none of
//! from a node identity the template never admits there.

use core::str::FromStr;

use openehr_base::v1_2::base_types::identification::archetype_id::ArchetypeId;
use openehr_rm::v1_2::model;
use openehr_rm::v1_2::paths::RmPath;
use openehr_sdt::flat::webtemplate::model::WebTemplateNode;
use regex::Regex;

use crate::index::matching::parse_aql_path;
use crate::template::PathError;

/// An `ARCHETYPE_SLOT` a template keeps open under one attribute (AOM 1.4
/// `ARCHETYPE_SLOT`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSlot {
    /// The absolute path of the attribute the slot constrains, its last
    /// segment the attribute with no predicate.
    attribute: RmPath,
    /// The reference-model class a filler conforms to, empty for any.
    rm_type: String,
    /// The archetype-id patterns a filler matches at least one of.
    includes: Vec<String>,
    /// The archetype-id patterns a filler matches none of.
    excludes: Vec<String>,
}

impl OpenSlot {
    /// Returns the absolute path of the attribute the slot constrains.
    #[must_use]
    pub const fn attribute(&self) -> &RmPath {
        &self.attribute
    }

    /// Returns whether the slot admits an archetype of id `archetype_id`.
    ///
    /// The archetype's reference-model class conforms to the slot's, and the
    /// id matches at least one include pattern, or the slot has none, and
    /// matches no exclude pattern (AOM 1.4 `ARCHETYPE_SLOT`, `includes` and
    /// `excludes`). A match-all exclude beside a non-empty include list is the
    /// ADL 1.4 closed-slot idiom and excludes nothing the includes admit. A
    /// pattern the `regex` crate cannot compile admits nothing: an include
    /// matches no id and an exclude matches every id, so an unreadable slot
    /// never makes a node read as one the template leaves out.
    #[must_use]
    pub fn admits(&self, archetype_id: &str) -> bool {
        let Ok(id) = ArchetypeId::from_str(archetype_id) else {
            return false;
        };
        if !self.rm_type.is_empty() && !model::is_a(id.rm_entity(), &self.rm_type) {
            return false;
        }
        let included = self.includes.is_empty()
            || self
                .includes
                .iter()
                .any(|pattern| full_match(pattern, archetype_id) == Some(true));
        included
            && !self.excludes.iter().any(|pattern| {
                let closed_idiom =
                    !self.includes.is_empty() && matches!(pattern.trim(), ".*" | ".+");
                !closed_idiom && full_match(pattern, archetype_id) != Some(false)
            })
    }
}

/// Returns whether `pattern` matches the whole of `value`, `None` when the
/// pattern does not compile.
///
/// An ADL 1.4 slot assertion writes its pattern between slashes
/// (`archetype_id/value matches {/openEHR-EHR-CLUSTER\.address\.v1/}`), and
/// the pattern constrains the whole archetype id.
fn full_match(pattern: &str, value: &str) -> Option<bool> {
    let body = pattern
        .strip_prefix('/')
        .and_then(|inner| inner.strip_suffix('/'))
        .unwrap_or(pattern);
    Regex::new(&format!("^(?:{body})$"))
        .ok()
        .map(|regex| regex.is_match(value))
}

/// Reads every open slot of `tree` and of every node under it.
///
/// # Errors
///
/// Returns [`PathError::MalformedAqlPath`] when the builder recorded an
/// attribute path this crate cannot read.
pub(super) fn read(tree: &WebTemplateNode) -> Result<Vec<OpenSlot>, PathError> {
    let mut slots = Vec::new();
    let mut stack = vec![tree];
    while let Some(node) = stack.pop() {
        for attribute in &node.closed_attributes {
            if attribute.slots.is_empty() {
                continue;
            }
            let path = parse_aql_path(&attribute.path)?;
            for slot in &attribute.slots {
                slots.push(OpenSlot {
                    attribute: path.clone(),
                    rm_type: slot.rm_type.clone(),
                    includes: slot.includes.clone(),
                    excludes: slot.excludes.clone(),
                });
            }
        }
        stack.extend(node.children.iter().chain(&node.alternatives));
    }
    Ok(slots)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(includes: &[&str], excludes: &[&str]) -> OpenSlot {
        OpenSlot {
            attribute: RmPath {
                absolute: true,
                segments: Vec::new(),
            },
            rm_type: String::from("CLUSTER"),
            includes: includes.iter().map(|&p| p.to_owned()).collect(),
            excludes: excludes.iter().map(|&p| p.to_owned()).collect(),
        }
    }

    #[test]
    fn an_include_pattern_admits_the_whole_id_only() {
        let names = slot(
            &[r"openEHR-EHR-CLUSTER\.structured_name(-[a-zA-Z0-9_]+)*\.v1"],
            &[],
        );
        assert!(names.admits("openEHR-EHR-CLUSTER.structured_name.v1"));
        assert!(names.admits("openEHR-EHR-CLUSTER.structured_name-de.v1"));
        assert!(!names.admits("openEHR-EHR-CLUSTER.structured_name.v10"));
        assert!(!names.admits("openEHR-EHR-CLUSTER.address.v1"));
    }

    #[test]
    fn a_match_all_exclude_beside_includes_is_the_closed_slot_idiom() {
        let closed = slot(&[r"openEHR-EHR-CLUSTER\.address\.v1"], &[".*"]);
        assert!(closed.admits("openEHR-EHR-CLUSTER.address.v1"));
        assert!(!closed.admits("openEHR-EHR-CLUSTER.person.v1"));
    }

    #[test]
    fn an_exclude_pattern_removes_what_it_matches() {
        let open = slot(&[], &[r"openEHR-EHR-CLUSTER\.device\.v1"]);
        assert!(open.admits("openEHR-EHR-CLUSTER.address.v1"));
        assert!(!open.admits("openEHR-EHR-CLUSTER.device.v1"));
    }

    #[test]
    fn a_filler_of_another_class_is_not_admitted() {
        let any = slot(&[".*"], &[]);
        assert!(any.admits("openEHR-EHR-CLUSTER.device.v1"));
        assert!(!any.admits("openEHR-EHR-OBSERVATION.blood_pressure.v2"));
        assert!(!any.admits("not an archetype id"));
    }

    #[test]
    fn a_pattern_that_does_not_compile_admits_nothing() {
        assert!(!slot(&["("], &[]).admits("openEHR-EHR-CLUSTER.address.v1"));
        assert!(!slot(&[], &["("]).admits("openEHR-EHR-CLUSTER.address.v1"));
    }
}
