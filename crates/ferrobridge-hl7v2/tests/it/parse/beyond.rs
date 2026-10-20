// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Components, subcomponents and repetitions counted beyond their definition.

use ferrobridge_hl7v2::parse::{Parsed, Unplaced};

use crate::{fixtures, support};

/// A v2.9.1 ADT^A01 whose PID carries `pid`, so its fields read against the
/// v2.9.1 segment table and data types.
fn adt_a01_with_pid(pid: &[u8]) -> Vec<u8> {
    fixtures::message(&[
        b"MSH|^~\\&|ADMIT|NORTHHOSP|EHR|SOUTHCLINIC|20261019090000+0200||ADT^A01^ADT_A01|MSG0295|P|2.9.1",
        b"EVN||20261019085900+0200",
        pid,
        b"PV1|1|I",
    ])
}

/// The constituents counted beyond their definition, as kind, ERL location
/// and the field id or data type named.
fn beyond(parsed: &Parsed) -> Vec<(&'static str, String, &'static str)> {
    parsed
        .unplaced()
        .iter()
        .filter_map(|outcome| match outcome {
            Unplaced::ExtraRepetition { location, field } => {
                Some((outcome.kind(), location.erl('^'), *field))
            }
            Unplaced::ExtraComponent {
                location,
                data_type,
            }
            | Unplaced::ExtraSubcomponent {
                location,
                data_type,
            } => Some((outcome.kind(), location.erl('^'), *data_type)),
            _ => None,
        })
        .collect()
}

// HL7 v2 chapter 2 §2.6, Rules for the recipient: ignore "components, subcomponents, and
// extra repetitions of a field that are present but were not expected".
#[test]
fn a_valued_component_past_the_data_type_is_counted_and_the_message_parses() {
    let parsed = support::parsed(&adt_a01_with_pid(
        b"PID|1||PAT-0295^^^NORTHHOSP^MR^^^^^^^^EXTRA||Test^Anna",
    ));
    assert!(parsed.refusals().is_empty(), "{:?}", parsed.refusals());
    assert_eq!(
        beyond(&parsed),
        [("extra-component", String::from("PID^1^3^1^13"), "CX")]
    );
    let kept = parsed
        .message()
        .segments()
        .get(2)
        .and_then(|pid| pid.field(3))
        .and_then(|field| field.repetitions().first())
        .and_then(|repetition| repetition.component(13))
        .and_then(|component| component.text());
    assert_eq!(kept, Some("EXTRA"), "the value stays reachable");
}

#[test]
fn a_valued_subcomponent_past_the_component_s_data_type_is_counted() {
    let parsed = support::parsed(&adt_a01_with_pid(
        b"PID|1||PAT-0295^^^NORTHHOSP&1.2.3&ISO&EXTRA^MR||Test^Anna",
    ));
    assert!(parsed.refusals().is_empty(), "{:?}", parsed.refusals());
    assert_eq!(
        beyond(&parsed),
        [("extra-subcomponent", String::from("PID^1^3^1^4^4"), "HD")]
    );
}

#[test]
fn a_valued_subcomponent_of_a_primitive_component_is_counted() {
    let parsed = support::parsed(&adt_a01_with_pid(
        b"PID|1||PAT-0295&EXTRA^^^NORTHHOSP^MR||Test^Anna",
    ));
    assert_eq!(
        beyond(&parsed),
        [("extra-subcomponent", String::from("PID^1^3^1^1^2"), "ST")]
    );
}

#[test]
fn a_valued_repetition_past_the_field_s_cardinality_is_counted() {
    let parsed = support::parsed(&adt_a01_with_pid(
        b"PID|1||PAT-0295^^^NORTHHOSP^MR||Test^Anna|||F~M",
    ));
    assert!(parsed.refusals().is_empty(), "{:?}", parsed.refusals());
    assert_eq!(
        beyond(&parsed),
        [(
            "extra-repetition",
            String::from("PID^1^8^2"),
            "PID.8-administrativeSex"
        )]
    );
}

#[test]
fn empty_trailing_constituents_are_not_counted() {
    let parsed = support::parsed(&adt_a01_with_pid(
        b"PID|1||PAT-0295^^^NORTHHOSP&&&^MR^^^^^^^^^^||Test^Anna|||F~",
    ));
    assert_eq!(beyond(&parsed), []);
}
