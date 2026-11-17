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
use openehr_rm::v1_2::paths::RmPath;
use openehr_sdt::flat::webtemplate::constraint::WebTemplateArchetypeSlot;
use openehr_sdt::flat::webtemplate::model::WebTemplateNode;

use crate::index::matching::parse_aql_path;
use crate::template::PathError;

/// An `ARCHETYPE_SLOT` a template keeps open under one attribute (AOM 1.4
/// `ARCHETYPE_SLOT`).
#[derive(Debug, Clone)]
pub struct OpenSlot {
    /// The absolute path of the attribute the slot constrains, its last
    /// segment the attribute with no predicate.
    attribute: RmPath,
    /// The slot as the template states it.
    slot: WebTemplateArchetypeSlot,
}

impl OpenSlot {
    /// Returns the absolute path of the attribute the slot constrains.
    #[must_use]
    pub const fn attribute(&self) -> &RmPath {
        &self.attribute
    }

    /// Returns whether the slot admits an archetype of id `archetype_id`.
    ///
    /// The decision is `openehr-sdt`'s (`WebTemplateArchetypeSlot::admits`,
    /// AOM 1.4 `ARCHETYPE_SLOT`), with the filler's reference-model class read
    /// from the id. An id that is no archetype id is admitted by no slot.
    #[must_use]
    pub fn admits(&self, archetype_id: &str) -> bool {
        ArchetypeId::from_str(archetype_id)
            .is_ok_and(|id| self.slot.admits(id.rm_entity(), archetype_id))
    }
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
                    slot: slot.clone(),
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
            slot: WebTemplateArchetypeSlot {
                rm_type: String::from("CLUSTER"),
                min: 0,
                max: -1,
                includes: includes.iter().map(|&p| p.to_owned()).collect(),
                excludes: excludes.iter().map(|&p| p.to_owned()).collect(),
            },
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
