// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Field optionality held to the version MSH-12 selects.

use ferrobridge_hl7v2::parse::Unplaced;

use super::required_missing;
use crate::{fixtures, support};

/// A synthetic ADT^A01 at `version` with one DG1 valuing DG1-1, DG1-2 and
/// DG1-6 and leaving DG1-3 empty.
fn adt_with_dg1(version: &str) -> Vec<u8> {
    let header = format!(
        "MSH|^~\\&|ADMIT|NORTHHOSP|EHR|SOUTHCLINIC|20260926090000+0200||ADT^A01^ADT_A01|MSG00041|P|{version}"
    );
    fixtures::message(&[
        header.as_bytes(),
        b"EVN||20260926085900+0200",
        b"PID|1||PAT-0041^^^NORTHHOSP^MR||Test^Anna",
        b"PV1|1|I",
        b"DG1|1|I10||||A",
    ])
}

// NOTE: the IGAMT 2.5.1 fields.json marks DG1-1, DG1-2 and DG1-6 required and the v2.9.1
// DG1.json DG1-1, DG1-3 and DG1-6, so the counted field follows the version MSH-12 selects.
#[test]
fn a_2_5_1_message_is_held_to_the_2_5_1_field_optionality() {
    let parsed = support::parsed(&adt_with_dg1("2.5.1"));
    assert_eq!(
        (parsed.structure().id, parsed.structure().version),
        ("ADT_A01", "2.5.1")
    );
    assert_eq!(required_missing(&parsed), Vec::<String>::new());
    assert!(parsed.unplaced().contains(&Unplaced::VersionSelected {
        structure: "ADT_A01",
        version: "2.5.1",
        declared: Some(String::from("2.5.1")),
    }));
}

#[test]
fn the_same_message_at_2_9_1_is_held_to_the_field_2_9_1_requires() {
    let parsed = support::parsed(&adt_with_dg1("2.9.1"));
    assert_eq!(
        (parsed.structure().id, parsed.structure().version),
        ("ADT_A01-A", "2.9.1")
    );
    assert_eq!(
        required_missing(&parsed),
        ["DG1[1]-3 DG1.3-diagnosisCode is required"]
    );
    assert!(
        parsed
            .unplaced()
            .iter()
            .all(|outcome| outcome.kind() != "version-selected"),
        "{:?}",
        parsed.unplaced()
    );
}

#[test]
fn a_message_with_no_version_or_one_the_tables_lack_selects_v2_9_1_and_is_counted() {
    let without = fixtures::message(&[
        b"MSH|^~\\&|ADMIT|NORTHHOSP|EHR|SOUTHCLINIC|20260926090000+0200||ADT^A01^ADT_A01|MSG00042|P|",
        b"EVN||20260926085900+0200",
        b"PID|1||PAT-0042^^^NORTHHOSP^MR||Test^Anna",
        b"PV1|1|I",
    ]);
    let parsed = support::parsed(&without);
    assert_eq!(parsed.structure().version, "2.9.1");
    assert!(parsed.unplaced().contains(&Unplaced::VersionSelected {
        structure: "ADT_A01-A",
        version: "2.9.1",
        declared: None,
    }));
    let parsed = support::parsed(&adt_with_dg1("2.9"));
    assert_eq!(parsed.structure().version, "2.9.1");
    assert!(parsed.unplaced().contains(&Unplaced::VersionSelected {
        structure: "ADT_A01-A",
        version: "2.9.1",
        declared: Some(String::from("2.9")),
    }));
    assert!(parsed.unplaced().contains(&Unplaced::EarlierVersion {
        declared: String::from("2.9"),
    }));
}

// NOTE: the IGAMT 2.5.1 fields.json and the v2.9.1 RXA.json both mark RXA-1 to RXA-6 required and
// RXA-22 optional, so a 2.5.1 VXU^V04 without RXA-4 is counted under either version's tables.
#[test]
fn a_2_5_1_vxu_v04_is_parsed_against_the_2_5_1_tree_and_its_rxa_table() {
    let bytes = fixtures::message(&[
        b"MSH|^~\\&|IIS|NORTHCLINIC|REG|SOUTHREG|20260926090000+0200||VXU^V04^VXU_V04|MSG00043|P|2.5.1",
        b"PID|1||PAT-0043^^^NORTHCLINIC^MR||Test^Bram",
        b"ORC|RE||IMM-0043",
        b"RXA|0|1|20260901||21^Varicella^CVX|999",
    ]);
    let parsed = support::parsed(&bytes);
    assert_eq!(
        (parsed.structure().id, parsed.structure().version),
        ("VXU_V04", "2.5.1")
    );
    assert_eq!(required_missing(&parsed), ["RXA[1]-4 RXA.4 is required"]);
    assert!(parsed.refusals().is_empty(), "{:?}", parsed.refusals());
    let with_end = fixtures::message(&[
        b"MSH|^~\\&|IIS|NORTHCLINIC|REG|SOUTHREG|20260926090000+0200||VXU^V04^VXU_V04|MSG00044|P|2.5.1",
        b"PID|1||PAT-0044^^^NORTHCLINIC^MR||Test^Bram",
        b"ORC|RE||IMM-0044",
        b"RXA|0|1|20260901|20260901|21^Varicella^CVX|999",
    ]);
    let parsed = support::parsed(&with_end);
    assert_eq!(required_missing(&parsed), Vec::<String>::new());
}
