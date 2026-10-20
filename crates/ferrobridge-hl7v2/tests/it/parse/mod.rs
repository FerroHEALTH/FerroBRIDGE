// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Positional parsing and the grouping by the generated message structures.

mod beyond;
mod legacy;
mod lex;
mod optionality;
mod selection;

use ferrobridge_hl7v2::decode::Charset;
use ferrobridge_hl7v2::parse::lex::lex;
use ferrobridge_hl7v2::parse::structure::{StructureError, structure_for};
use ferrobridge_hl7v2::parse::{ErrorCode, Item, Parsed, Unplaced};

use crate::{fixtures, support};

/// Renders the group tree as indented lines of group names and segment ids.
fn tree(parsed: &Parsed) -> String {
    fn walk(parsed: &Parsed, items: &[Item], depth: usize, out: &mut String) {
        for item in items {
            match item {
                Item::Segment(placed) => {
                    let id = parsed.message().segments()[placed.index].id();
                    out.push_str(&"  ".repeat(depth));
                    out.push_str(id);
                    out.push('\n');
                }
                Item::Group(instance) => {
                    out.push_str(&"  ".repeat(depth));
                    out.push_str(instance.group.name);
                    out.push('[');
                    out.push_str(&instance.occurrence.to_string());
                    out.push_str("]\n");
                    walk(parsed, &instance.items, depth + 1, out);
                }
            }
        }
    }
    let mut out = String::new();
    walk(parsed, parsed.items(), 0, &mut out);
    out
}

#[test]
fn an_oru_r01_is_grouped_by_its_structure() {
    let parsed = support::parsed(&fixtures::oru_r01());
    assert_eq!(parsed.structure().id, "ORU_R01");
    assert_eq!(
        tree(&parsed),
        "MSH\n\
         PATIENT_RESULT[0]\n\
         \x20 PATIENT[0]\n\
         \x20   PID\n\
         \x20   VISIT[0]\n\
         \x20     PV1\n\
         \x20 ORDER_OBSERVATION[0]\n\
         \x20   ORC\n\
         \x20   OBR\n\
         \x20   OBSERVATION[0]\n\
         \x20     OBX\n\
         \x20   OBSERVATION[1]\n\
         \x20     OBX\n"
    );
    assert!(parsed.refusals().is_empty(), "{:?}", parsed.refusals());
}

#[test]
fn a_z_segment_is_a_counted_outcome() {
    let parsed = support::parsed(&fixtures::oru_r01());
    let unknown: Vec<String> = parsed
        .unplaced()
        .iter()
        .filter_map(|outcome| match outcome {
            Unplaced::UnknownSegment { location } => Some(location.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(unknown, ["ZXX[1]"]);
}

#[test]
fn a_2_5_1_sender_is_parsed_under_its_own_tables_and_counted() {
    let parsed = support::parsed(&fixtures::oru_r01());
    assert!(parsed.unplaced().contains(&Unplaced::VersionSelected {
        structure: "ORU_R01",
        version: "2.5.1",
        declared: Some(String::from("2.5.1")),
    }));
    assert!(
        !parsed
            .unplaced()
            .iter()
            .any(|outcome| matches!(outcome, Unplaced::EarlierVersion { .. })),
        "a 2.5.1 tree is the sender's own, so the message is not parsed against 2.9.1"
    );
}

#[test]
fn an_adt_a01_is_grouped_by_its_structure() {
    let parsed = support::parsed(&fixtures::adt_a01());
    assert_eq!(parsed.structure().id, "ADT_A01");
    assert_eq!(tree(&parsed), "MSH\nEVN\nPID\nPV1\n");
    assert!(parsed.refusals().is_empty(), "{:?}", parsed.refusals());
}

#[test]
fn a_missing_required_field_is_counted_at_its_location_and_the_message_parses() {
    let parsed = support::parsed(&fixtures::oru_r01_without_patient_name());
    assert!(parsed.refusals().is_empty(), "{:?}", parsed.refusals());
    let missing: Vec<(String, &str, &str, &str)> = parsed
        .unplaced()
        .iter()
        .filter_map(|outcome| match outcome {
            Unplaced::MissingRequiredField {
                location,
                field,
                segment,
                version,
            } => Some((location.erl('^'), *field, *segment, *version)),
            _ => None,
        })
        .collect();
    assert_eq!(
        missing,
        [(String::from("PID^1^5"), "PID.5", "PID", "2.5.1")]
    );
}

#[test]
fn an_empty_field_the_bridge_needs_to_answer_is_still_a_refusal() {
    // MSH-12 empty selects v2.9.1, whose MSH-12 is required.
    let bytes = fixtures::message(&[
        b"MSH|^~\\&|ADMIT|NORTHHOSP|EHR|SOUTHCLINIC|20260926090000+0200||ADT^A01^ADT_A01|MSG00045|P|",
        b"EVN||20260926085900+0200",
        b"PID|1||PAT-0045^^^NORTHHOSP^MR||Test^Anna",
        b"PV1|1|I",
    ]);
    let parsed = support::parsed(&bytes);
    let refusals: Vec<(String, ErrorCode)> = parsed
        .refusals()
        .iter()
        .map(|refusal| (refusal.location.erl('^'), refusal.code))
        .collect();
    assert_eq!(
        refusals,
        [(String::from("MSH^1^12"), ErrorCode::RequiredFieldMissing)]
    );
}

#[test]
fn a_missing_required_segment_is_a_segment_sequence_refusal() {
    let bytes = fixtures::message(&[
        b"MSH|^~\\&|ADMIT|NORTHHOSP|EHR|SOUTHCLINIC|20260925090000+0200||ADT^A01^ADT_A01|MSG1|P|2.5.1",
        b"EVN||20260925085900+0200",
        b"PID|1||PAT-0002^^^NORTHHOSP^MR||Doe^Sam",
    ]);
    let parsed = support::parsed(&bytes);
    assert_eq!(
        parsed
            .refusals()
            .iter()
            .map(|refusal| (refusal.code, refusal.detail.as_str()))
            .collect::<Vec<_>>(),
        [(
            ErrorCode::SegmentSequence,
            "the required segment ADT_A01.8-PV1 is missing"
        )]
    );
}

#[test]
fn a_known_segment_out_of_its_place_is_counted() {
    let bytes = fixtures::message(&[
        b"MSH|^~\\&|ADMIT|NORTHHOSP|EHR|SOUTHCLINIC|20260925090000+0200||ADT^A01^ADT_A01|MSG1|P|2.5.1",
        b"EVN||20260925085900+0200",
        b"PID|1||PAT-0002^^^NORTHHOSP^MR||Doe^Sam",
        b"PV1|1|I",
        b"MSA|AA|MSG0",
    ]);
    let parsed = support::parsed(&bytes);
    assert!(parsed.unplaced().iter().any(|outcome| matches!(
        outcome,
        Unplaced::OutOfStructure { location } if location.segment == "MSA"
    )));
}

#[test]
fn a_valued_field_beyond_the_segment_table_is_counted() {
    let mut fields = String::from("PV1|1|I");
    fields.push_str(&"|".repeat(60));
    fields.push_str("EXTRA");
    let bytes = fixtures::message(&[
        b"MSH|^~\\&|ADMIT|NORTHHOSP|EHR|SOUTHCLINIC|20260925090000+0200||ADT^A01^ADT_A01|MSG1|P|2.5.1",
        b"EVN||20260925085900+0200",
        b"PID|1||PAT-0002^^^NORTHHOSP^MR||Doe^Sam",
        fields.as_bytes(),
    ]);
    let parsed = support::parsed(&bytes);
    assert!(parsed.unplaced().iter().any(|outcome| matches!(
        outcome,
        Unplaced::ExtraField { location } if location.segment == "PV1"
    )));
}

/// Lexes `bytes` as ASCII and selects its structure.
fn select(bytes: &[u8]) -> Result<&'static str, StructureError> {
    let decoded =
        ferrobridge_hl7v2::decode::decode(bytes, Charset::Ascii).expect("the message decodes");
    let lexed = lex(&decoded.text, decoded.charset).expect("it lexes");
    structure_for(&lexed.message).map(|structure| structure.id)
}

/// A synthetic header with `message_type` as MSH-9, and an EVN, a PID and a PV1.
fn with_type(message_type: &str) -> Vec<u8> {
    with_type_at(message_type, "2.5.1")
}

/// The header of [`with_type`] declaring `version` as MSH-12.
fn with_type_at(message_type: &str, version: &str) -> Vec<u8> {
    let header = format!(
        "MSH|^~\\&|ADMIT|NORTHHOSP|EHR|SOUTHCLINIC|20260925090000+0200||{message_type}|MSG00009|P|{version}"
    );
    fixtures::message(&[
        header.as_bytes(),
        b"EVN||20260925085900+0200",
        b"PID|1||PAT-0009^^^NORTHHOSP^MR||Test^Anna^^^^^L||19920304|F",
        b"PV1|1|O",
    ])
}

/// The required-field refusals of `parsed`, as `location detail`.
fn required_missing(parsed: &Parsed) -> Vec<String> {
    parsed
        .unplaced()
        .iter()
        .filter_map(|outcome| match outcome {
            Unplaced::MissingRequiredField {
                location, field, ..
            } => Some(format!("{location} {field} is required")),
            _ => None,
        })
        .collect()
}
