// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The per-version structures of the legacy tables, the vendored ORM^O01
//! messages and the structures v2.9.1 withdrew.

use std::collections::BTreeMap;

use ferrobridge_hl7v2::decode::Charset;
use ferrobridge_hl7v2::parse::Unplaced;
use ferrobridge_hl7v2::parse::lex::lex;
use ferrobridge_hl7v2::parse::structure::{StructureError, structure_for};

use super::{required_missing, select, with_type_at};
use crate::{fixtures, support};

/// The vendored ORM^O01 messages that decode, by the path under `vendor/` and
/// the version their MSH-12 declares.
const ORM_O01_MESSAGES: [(&str, &str); 11] = [
    ("fhir-converter/data/SampleData/Hl7v2/LAB-ORM-1.hl7", "2.3"),
    (
        "reportstream/prime-router/src/testIntegration/resources/datatests/HL7_to_FHIR/sample_orm_20230809-001.hl7",
        "2.5.1",
    ),
    (
        "reportstream/prime-router/src/testIntegration/resources/datatests/mappinginventory/catchall/ormo01/orm_o01-full.hl7",
        "2.5.1",
    ),
    (
        "reportstream/prime-router/src/testIntegration/resources/datatests/mappinginventory/catchall/orcobr/orm-obr-to-specimen.hl7",
        "2.5.1",
    ),
    ("fhir-converter/data/SampleData/Hl7v2/ORM-O01-01.hl7", "2.6"),
    ("fhir-converter/data/SampleData/Hl7v2/ORM-O01-02.hl7", "2.6"),
    ("fhir-converter/data/SampleData/Hl7v2/ORM-O01-03.hl7", "2.6"),
    ("fhir-converter/data/SampleData/Hl7v2/ORM-O01-04.hl7", "2.6"),
    ("fhir-converter/data/SampleData/Hl7v2/ORM-O01-05.hl7", "2.6"),
    ("fhir-converter/data/SampleData/Hl7v2/ORM-O01-06.hl7", "2.6"),
    ("fhir-converter/data/SampleData/Hl7v2/OML-O21-02.hl7", "2.6"),
];

/// The file at `path` under `vendor/` as a frame carries it: the byte order
/// mark removed and every line ending a carriage return.
fn vendored(path: &str) -> Vec<u8> {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor")
            .join(path),
    )
    .expect("the vendored message reads");
    let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let mut out = Vec::new();
    for line in text.split(|byte| *byte == b'\r' || *byte == b'\n') {
        if !line.iter().all(u8::is_ascii_whitespace) {
            out.extend_from_slice(line);
            out.push(b'\r');
        }
    }
    out
}

#[test]
fn the_vendored_orm_o01_messages_parse_against_the_tree_of_their_version() {
    // Each counted field is one the tables of the declared version make required
    // (ORC-7 at 2.3, AL1-1 at 2.6) and the message leaves empty.
    let refused: BTreeMap<&str, Vec<&str>> = BTreeMap::from([
        (ORM_O01_MESSAGES[0].0, vec!["ORC[1]-7 ORC.7 is required"]),
        (
            ORM_O01_MESSAGES[3].0,
            vec![
                "MSH[1]-11 MSH.11 is required",
                "PID[1]-3 PID.3 is required",
                "PID[1]-5 PID.5 is required",
                "OBR[1]-4 OBR.4 is required",
            ],
        ),
        (ORM_O01_MESSAGES[5].0, vec!["AL1[1]-1 AL1.1 is required"]),
    ]);
    for (path, version) in ORM_O01_MESSAGES {
        let parsed = support::parsed(&vendored(path));
        assert_eq!(parsed.structure().id, "ORM_O01", "{path}");
        assert_eq!(parsed.structure().version, version, "{path}");
        assert_eq!(parsed.structure().url, None, "{path}");
        assert!(
            parsed.refusals().is_empty(),
            "{path}: {:?}",
            parsed.refusals()
        );
        let expected = refused.get(path).cloned().unwrap_or_default();
        assert_eq!(required_missing(&parsed), expected, "{path}");
    }
}

#[test]
fn the_vendored_2_3_1_orm_o01_is_refused_before_selection_for_its_character_set() {
    // MSH-18 declares ASCII and the message carries a byte outside it.
    let bytes = vendored("v2-to-fhir/derived/ORM_O01.hl7");
    assert!(matches!(
        ferrobridge_hl7v2::decode::decode(&bytes, Charset::Ascii),
        Err(ferrobridge_hl7v2::decode::DecodeError::Undeclared { .. })
    ));
}

#[test]
fn a_withdrawn_structure_is_a_counted_outcome_naming_its_versions() {
    let parsed = support::parsed(&vendored(ORM_O01_MESSAGES[4].0));
    // The IGAMT export lists ORM_O01 at 2.3 to 2.6 and not at 2.7.
    assert!(parsed.unplaced().contains(&Unplaced::WithdrawnStructure {
        structure: "ORM_O01",
        version: "2.6",
        withdrawn_as_of: "2.7",
        declared: Some(String::from("2.6")),
    }));
    assert!(
        !parsed
            .unplaced()
            .iter()
            .any(|outcome| matches!(outcome, Unplaced::EarlierVersion { .. })),
        "a legacy tree is its own version's, so the message is not parsed against 2.9.1"
    );
    assert_eq!(
        parsed
            .unplaced()
            .iter()
            .filter(|outcome| outcome.kind() == "withdrawn-structure")
            .count(),
        1
    );
}

/// A synthetic ORM^O01 at `version`, MSH-9 as `message_type`.
fn orm(message_type: &str, version: &str) -> Vec<u8> {
    let header = format!(
        "MSH|^~\\&|ORDERS|NORTHHOSP|LAB|SOUTHLAB|20260925090000+0200||{message_type}|MSG00031|P|{version}"
    );
    fixtures::message(&[
        header.as_bytes(),
        b"PID|1||PAT-0031^^^NORTHHOSP^MR||Test^Bram||19800101|M",
        b"ORC|NW|PLC-0031",
        b"OBR|1|PLC-0031||GLU^Glucose^L",
    ])
}

#[test]
fn a_withdrawn_structure_is_selected_by_msh_9_3_and_the_declared_version() {
    let selected = |bytes: &[u8]| {
        let decoded =
            ferrobridge_hl7v2::decode::decode(bytes, Charset::Ascii).expect("the message decodes");
        let lexed = lex(&decoded.text, decoded.charset).expect("it lexes");
        structure_for(&lexed.message).map(|structure| (structure.id, structure.version))
    };
    assert_eq!(
        selected(&orm("ORM^O01^ORM_O01", "2.4")),
        Ok(("ORM_O01", "2.4"))
    );
    assert_eq!(selected(&orm("ORM^O01", "2.3.1")), Ok(("ORM_O01", "2.3.1")));
}

#[test]
fn a_version_the_tables_lack_takes_the_nearest_earlier_tree() {
    // The export carries ORM_O01 up to 2.6, so a 2.8 sender is parsed against 2.6.
    let parsed = support::parsed(&orm("ORM^O01^ORM_O01", "2.8"));
    assert_eq!(parsed.structure().version, "2.6");
    assert!(parsed.unplaced().contains(&Unplaced::WithdrawnStructure {
        structure: "ORM_O01",
        version: "2.6",
        withdrawn_as_of: "2.7",
        declared: Some(String::from("2.8")),
    }));
}

#[test]
fn a_withdrawn_structure_before_its_first_tree_is_refused_with_the_versions() {
    assert_eq!(
        select(&orm("ORM^O01^ORM_O01", "2.2")),
        Err(StructureError::NoLegacyVersion {
            name: String::from("ORM_O01"),
            declared: Some(String::from("2.2")),
            versions: vec!["2.3", "2.3.1", "2.4", "2.5", "2.5.1", "2.6"],
        })
    );
    assert!(matches!(
        select(&orm("ORM^O01", "2.x")),
        Err(StructureError::NoLegacyVersion { .. })
    ));
}

// NOTE: HL7 v2.5.1 chapter 2 §2.15.9.9; the 2.8 interactions of the IGAMT export send REF_I12
// under the event I14, and HL7/v2ig input/sourceOfTruth defines no REF message.
#[test]
fn a_code_and_event_only_the_legacy_interactions_send_select_their_structure() {
    let selected = |message_type: &str| {
        let bytes = with_type_at(message_type, "2.8");
        let decoded =
            ferrobridge_hl7v2::decode::decode(&bytes, Charset::Ascii).expect("the message decodes");
        let lexed = lex(&decoded.text, decoded.charset).expect("it lexes");
        structure_for(&lexed.message).map(|structure| (structure.id, structure.version))
    };
    assert!(hl7v2_types::message::find("REF", "I14").is_none());
    assert_eq!(selected("REF^I14^REF_I14"), Ok(("REF_I12", "2.8")));
    assert_eq!(selected("REF^I14"), Ok(("REF_I12", "2.8")));
    assert_eq!(selected("RRI^I14^RRI_I14"), Ok(("RRI_I12", "2.8")));
    let parsed = support::parsed(&with_type_at("REF^I14^REF_I14", "2.8"));
    assert!(
        parsed.unplaced().contains(&Unplaced::OtherStructure {
            declared: String::from("REF_I14"),
            structure: "REF_I12",
        }),
        "{:?}",
        parsed.unplaced()
    );
}

#[test]
fn a_code_and_event_no_index_sends_leaves_an_unknown_structure_unknown() {
    assert_eq!(
        select(&with_type_at("REF^I99^REF_I99", "2.8")),
        Err(StructureError::Unknown {
            name: String::from("REF_I99"),
        })
    );
}

#[test]
fn a_structure_neither_the_definitions_nor_the_legacy_tables_carry_is_unknown() {
    assert_eq!(
        select(&orm("ORM^O01^ORM_Z99", "2.5.1")),
        Err(StructureError::Unknown {
            name: String::from("ORM_Z99"),
        })
    );
}
