// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! The examples of the crate's `README.md`, run as written. The crate builds
//! with `doctest = false`, so the README's code blocks run here instead.

use hl7v2_types::model::{Max, Node, Optionality};

#[test]
fn a_message_structure_leads_to_a_field_definition() {
    let adt_a04 = hl7v2_types::message::find("ADT", "A04").expect("a defined message");
    let structure = adt_a04.structure.expect("ADT^A04 names a structure");
    assert_eq!(structure.id, "ADT_A01-B");

    let pid = structure
        .nodes
        .iter()
        .find_map(|node| match node {
            Node::Segment(placed) if placed.segment.id == "PID" => Some(placed.segment),
            _ => None,
        })
        .expect("ADT_A01-B places PID at the top level");
    let identifiers = pid
        .fields
        .iter()
        .find(|field| field.position == 3)
        .expect("PID-3");
    assert_eq!(identifiers.name, "Patient Identifier List");
    assert_eq!(identifiers.optionality, Optionality::R);
    assert_eq!(identifiers.cardinality.max, Max::Unbounded);
    assert_eq!(
        identifiers.data_type.map(|data_type| data_type.code()),
        Some("CX")
    );
}

#[test]
fn a_withdrawn_structure_is_found_by_its_version() {
    let orm = hl7v2_types::message::find_legacy("ORM", "O01", "2.5.1").expect("carried at 2.5.1");
    assert_eq!(orm.structure.id, "ORM_O01");
    assert_eq!(orm.structure.withdrawn_as_of, Some("2.7"));
}
