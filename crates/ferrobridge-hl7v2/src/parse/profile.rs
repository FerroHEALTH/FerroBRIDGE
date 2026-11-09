// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The constraints a profile places on a base message structure.
//!
//! An integration profile defines its messages over an HL7 v2 structure and
//! may set a member's minimum cardinality apart from the base: IHE ITI TF-2
//! Table 3.21-6 makes the QRI of the ITI-21 response optional where the v2.5
//! `RSP_K21` requires it. A [`Profile`] carries such constraints as data, each
//! naming a node of a structure version by its generated id, and the grouping
//! reads a member's minimum through it. A profile may also set the
//! repetition of a field of a segment in a structure: ITI TF-2 §3.21.4.1.2.3
//! makes QPD-3 repeat where the v2.5 QPD gives it one repetition, and the
//! arity check reads a field's maximum through it. A profile never adds,
//! removes or reorders a node. No specification governs this representation:
//! our own design.

use hl7v2_types::model::{Cardinality, Field, Max};

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

/// One field whose repetition a profile sets, in the segments of one structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repetition {
    /// The HL7 version of the structure (`2.5`).
    pub version: &'static str,
    /// The structure id (`RSP_K21`).
    pub structure: &'static str,
    /// The field id (`QPD.3`).
    pub field: &'static str,
    /// The maximum number of repetitions the profile sets.
    pub max: Max,
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
    /// The repetitions, each of one field in one structure.
    pub repetitions: &'static [Repetition],
}

impl Profile {
    /// The base structure, with no constraint of a profile.
    pub const BASE: Self = Self {
        name: "base",
        source: "the HL7 v2 message structure",
        constraints: &[],
        repetitions: &[],
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

    /// Returns the maximum repetition of `field` in a segment of structure
    /// `structure` of `version`: the profile's when it sets the field's, else
    /// the field's own.
    #[must_use]
    pub fn max(&self, version: &str, structure: &str, field: &Field) -> Max {
        self.repetitions
            .iter()
            .find(|repetition| {
                repetition.version == version
                    && repetition.structure == structure
                    && repetition.field == field.id
            })
            .map_or(field.cardinality.max, |repetition| repetition.max)
    }
}

/// IHE ITI-21, Patient Demographics Query: the response's QRI is optional, and
/// QPD-3 repeats in the query and in the response that echoes it.
// NOTE: IHE ITI TF-2 §3.21.4.2.2 Table 3.21-6 gives the query response group as
// `[ { PID [ PD1 ] [ QRI ] } ]`, and §3.21.4.2.2.6 lets the Supplier leave QRI out.
pub const IHE_ITI_21: Profile = Profile {
    name: "IHE ITI-21 Patient Demographics Query",
    source: "IHE ITI TF-2 §3.21.4.1.2.3 and §3.21.4.2.2, Table 3.21-6",
    constraints: &[Constraint {
        version: "2.5",
        node: "RSP_K21.7-QUERY_RESPONSE.4-QRI",
        min: 0,
    }],
    // NOTE: IHE ITI TF-2 §3.21.4.1.2.3: QPD-3 "consists of one or more repetitions", one per
    // query parameter, and the RSP^K22 echoes the QPD (§3.21.4.2.2, Table 3.21-6).
    repetitions: &[
        Repetition {
            version: "2.5",
            structure: "QBP_Q21",
            field: "QPD.3",
            max: Max::Unbounded,
        },
        Repetition {
            version: "2.5",
            structure: "RSP_K21",
            field: "QPD.3",
            max: Max::Unbounded,
        },
    ],
};

#[cfg(test)]
mod tests {
    use hl7v2_types::model::{Max, Node, Segment, Structure};

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
    fn every_repetition_names_a_field_of_a_segment_its_structure_places() {
        for profile in [IHE_ITI_21] {
            for repetition in profile.repetitions {
                let placed = structure(repetition.structure, repetition.version);
                assert_eq!(placed.id, repetition.structure);
                let mut found = false;
                segments(placed.nodes, &mut |segment| {
                    found |= segment
                        .fields
                        .iter()
                        .any(|field| field.id == repetition.field);
                });
                assert!(
                    found,
                    "{} names no field of {}",
                    profile.name, repetition.structure
                );
            }
        }
    }

    #[test]
    fn qpd_3_repeats_under_iti_21_and_qpd_2_keeps_its_base() {
        let qpd = hl7v2_types::legacy::find("RSP_K21", "2.5").expect("the 2.5 RSP_K21");
        let mut fields = Vec::new();
        segments(qpd.nodes, &mut |segment| {
            if segment.id == "QPD" {
                fields.extend(segment.fields.iter().filter(|field| field.position <= 3));
            }
        });
        let [one, two, three] = fields.as_slice() else {
            panic!("QPD-1 to QPD-3: {fields:?}");
        };
        assert_eq!(three.cardinality.max, Max::Bounded(1), "the v2.5 base");
        assert_eq!(IHE_ITI_21.max("2.5", "RSP_K21", three), Max::Unbounded);
        assert_eq!(IHE_ITI_21.max("2.5", "QBP_Q21", three), Max::Unbounded);
        assert_eq!(IHE_ITI_21.max("2.5.1", "RSP_K21", three), Max::Bounded(1));
        assert_eq!(IHE_ITI_21.max("2.5", "RSP_K23", three), Max::Bounded(1));
        assert_eq!(Profile::BASE.max("2.5", "RSP_K21", three), Max::Bounded(1));
        for field in [one, two] {
            assert_eq!(
                IHE_ITI_21.max("2.5", "RSP_K21", field),
                field.cardinality.max
            );
        }
    }

    fn segments(nodes: &'static [Node], each: &mut impl FnMut(&'static Segment)) {
        for node in nodes {
            match node {
                Node::Segment(reference) => each(reference.segment),
                Node::Group(group) => segments(group.children, each),
                Node::Placeholder(_) => {}
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
            max: Max::Bounded(1),
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
