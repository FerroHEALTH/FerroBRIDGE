// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The constraints a profile places on a base message structure.
//!
//! An integration profile defines its messages over an HL7 v2 structure and
//! may set a member's minimum cardinality apart from the base: IHE ITI TF-2
//! Table 3.21-6 makes the QRI of the ITI-21 response optional where the v2.5
//! `RSP_K21` requires it. A [`Profile`] carries such constraints as data, each
//! naming a node of a structure version by its generated id, and the grouping
//! reads a member's minimum through it. A profile sets minimums only: it
//! never adds, removes or reorders a node, and never changes a maximum. No
//! specification governs this representation: our own design.

use hl7v2_types::model::Cardinality;

/// One member whose minimum cardinality a profile sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Constraint {
    /// The HL7 version of the structure the node belongs to (`2.5`).
    pub version: &'static str,
    /// The generated node id (`RSP_K21.7-QUERY_RESPONSE.4-QRI`).
    pub node: &'static str,
    /// The minimum the profile sets.
    pub min: u32,
}

/// The constraints of one profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    /// The profile's name.
    pub name: &'static str,
    /// The specification section the constraints come from.
    pub source: &'static str,
    /// The constraints, each on one node.
    pub constraints: &'static [Constraint],
}

impl Profile {
    /// The base structure, with no constraint of a profile.
    pub const BASE: Self = Self {
        name: "base",
        source: "the HL7 v2 message structure",
        constraints: &[],
    };

    /// Returns the minimum of node `node` of a structure of `version`: the
    /// profile's when it constrains the node, else `cardinality`'s.
    #[must_use]
    pub fn min(&self, version: &str, node: &str, cardinality: Cardinality) -> u32 {
        self.constraints
            .iter()
            .find(|constraint| constraint.version == version && constraint.node == node)
            .map_or(cardinality.min, |constraint| constraint.min)
    }
}

/// IHE ITI-21, Patient Demographics Query: the response's QRI is optional.
// NOTE: IHE ITI TF-2 §3.21.4.2.2 Table 3.21-6 gives the query response group as
// `[ { PID [ PD1 ] [ QRI ] } ]`, and §3.21.4.2.2.6 lets the Supplier leave QRI out.
pub const IHE_ITI_21: Profile = Profile {
    name: "IHE ITI-21 Patient Demographics Query",
    source: "IHE ITI TF-2 §3.21.4.2.2, Table 3.21-6",
    constraints: &[Constraint {
        version: "2.5",
        node: "RSP_K21.7-QUERY_RESPONSE.4-QRI",
        min: 0,
    }],
};

#[cfg(test)]
mod tests {
    use hl7v2_types::model::{Node, Structure};

    use super::{IHE_ITI_21, Profile};

    /// Whether `nodes` hold a node with `id`.
    fn holds(nodes: &'static [Node], id: &str) -> bool {
        nodes.iter().any(|node| match node {
            Node::Segment(reference) => reference.id == id,
            Node::Group(group) => group.id == id || holds(group.children, id),
            Node::Placeholder(_) => false,
        })
    }

    fn structure(name: &str, version: &str) -> &'static Structure {
        hl7v2_types::legacy::find(name, version).expect("the legacy structure")
    }

    #[test]
    fn every_constraint_names_a_node_of_its_structure_version() {
        for profile in [IHE_ITI_21] {
            for constraint in profile.constraints {
                let name = constraint
                    .node
                    .split_once('.')
                    .map(|(name, _)| name)
                    .expect("a node id names its structure");
                assert!(
                    holds(structure(name, constraint.version).nodes, constraint.node),
                    "{} names no node of {name} {}",
                    profile.name,
                    constraint.version
                );
            }
        }
    }

    #[test]
    fn a_profile_sets_the_minimum_of_its_node_and_no_other() {
        let rsp = structure("RSP_K21", "2.5");
        let mut seen = 0;
        visit(rsp.nodes, &mut |id, cardinality| {
            let min = IHE_ITI_21.min("2.5", id, cardinality);
            if id == "RSP_K21.7-QUERY_RESPONSE.4-QRI" {
                assert_eq!((cardinality.min, min), (1, 0));
                seen += 1;
            } else {
                assert_eq!(min, cardinality.min, "{id}");
            }
            assert_eq!(Profile::BASE.min("2.5", id, cardinality), cardinality.min);
        });
        assert_eq!(seen, 1);
        let qri = hl7v2_types::model::Cardinality {
            min: 1,
            max: hl7v2_types::model::Max::Bounded(1),
        };
        assert_eq!(
            IHE_ITI_21.min("2.5.1", "RSP_K21.7-QUERY_RESPONSE.4-QRI", qri),
            1,
            "another version keeps its base"
        );
    }

    fn visit(nodes: &'static [Node], each: &mut impl FnMut(&str, hl7v2_types::model::Cardinality)) {
        for node in nodes {
            match node {
                Node::Segment(reference) => each(reference.id, reference.cardinality),
                Node::Group(group) => {
                    each(group.id, group.cardinality);
                    visit(group.children, each);
                }
                Node::Placeholder(_) => {}
            }
        }
    }
}
