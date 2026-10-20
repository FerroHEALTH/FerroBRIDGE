// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Structure selection from MSH-9 and the message definitions.

use ferrobridge_hl7v2::parse::Unplaced;
use ferrobridge_hl7v2::parse::structure::StructureError;

use super::{select, with_type, with_type_at};
use crate::{fixtures, support};

#[test]
fn the_message_definition_of_the_trigger_event_selects_the_variant() {
    // HL7/v2ig input/sourceOfTruth/message/messages/ORU-R01.json names ORU_R01-A; a 2.5.1
    // sender is parsed against the one ORU_R01 of its own tables.
    assert_eq!(select(&fixtures::oru_r01()), Ok("ORU_R01"));
    assert_eq!(
        select(&with_type_at("ORU^R01^ORU_R01", "2.9.1")),
        Ok("ORU_R01-A")
    );
}

#[test]
fn a_variant_other_than_the_first_is_chosen_from_the_message_table() {
    // HL7/v2ig input/sourceOfTruth/message/messages/ADT-A04.json names ADT_A01-B.
    assert_eq!(select(&with_type("ADT^A04^ADT_A01")), Ok("ADT_A01"));
    assert_eq!(
        select(&with_type_at("ADT^A04^ADT_A01", "2.9.1")),
        Ok("ADT_A01-B")
    );
}

#[test]
fn a_structure_in_variants_no_message_definition_names_is_refused() {
    assert!(matches!(
        select(&with_type("ORU^Z99^ORU_R01")),
        Err(StructureError::Variant { ref candidates, .. })
            if candidates == &["ORU_R01-A", "ORU_R01-B", "ORU_R01-C", "ORU_R01-D"]
    ));
}

// NOTE: HL7 v2.5.1 chapter 2 §2.15.9.9; HL7/v2ig input/sourceOfTruth/message/messages/
// ADT-A08.json names ADT_A01-C, which answers for the ADT_A08 no definition carries.
#[test]
fn an_unknown_structure_falls_back_to_the_message_definition_and_is_counted() {
    // The 2.5.1 sender takes the ADT_A01 of its own tables.
    let bytes = with_type("ADT^A08^ADT_A08");
    assert_eq!(select(&bytes), Ok("ADT_A01"));
    let parsed = support::parsed(&bytes);
    assert!(
        parsed.unplaced().contains(&Unplaced::OtherStructure {
            declared: String::from("ADT_A08"),
            structure: "ADT_A01",
        }),
        "{:?}",
        parsed.unplaced()
    );
    assert!(parsed.refusals().is_empty(), "{:?}", parsed.refusals());
    let current = with_type_at("ADT^A08^ADT_A08", "2.9.1");
    assert_eq!(select(&current), Ok("ADT_A01-C"));
    let parsed = support::parsed(&current);
    assert!(
        parsed.unplaced().contains(&Unplaced::OtherStructure {
            declared: String::from("ADT_A08"),
            structure: "ADT_A01-C",
        }),
        "{:?}",
        parsed.unplaced()
    );
}

#[test]
fn a_structure_named_as_declared_counts_no_other_structure() {
    let parsed = support::parsed(&with_type("ADT^A04^ADT_A01"));
    assert!(
        parsed
            .unplaced()
            .iter()
            .all(|outcome| !matches!(outcome, Unplaced::OtherStructure { .. })),
        "{:?}",
        parsed.unplaced()
    );
}

#[test]
fn an_unknown_structure_with_no_message_definition_is_refused() {
    // ORM_O01 has no v2.9.1 message definition but the legacy tables carry it
    // (#303), so the refusal needs a structure neither source carries.
    assert!(matches!(
        select(&with_type("ORM^O01^ORM_Z01")),
        Err(StructureError::Unknown { ref name }) if name == "ORM_Z01"
    ));
    assert_eq!(select(&with_type("ORM^O01^ORM_O01")), Ok("ORM_O01"));
}

#[test]
fn a_known_structure_contradicting_the_message_definition_is_refused() {
    // ADT^A08 names ADT_A01-C, which is no variant of the ADT_A05 MSH-9.3 names.
    assert!(matches!(
        select(&with_type("ADT^A08^ADT_A05")),
        Err(StructureError::Variant { .. })
    ));
}

#[test]
fn a_message_definition_naming_a_variant_of_another_structure_is_refused() {
    // ADT^A04 names ADT_A01-B, which is no variant of the ADT_A05 MSH-9.3 names.
    assert!(matches!(
        select(&with_type("ADT^A04^ADT_A05")),
        Err(StructureError::Variant { .. })
    ));
}
