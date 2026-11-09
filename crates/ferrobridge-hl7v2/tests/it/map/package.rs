// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The vendored package as loaded: its maps by kind and its conditions.

use std::collections::BTreeMap;

use ferrobridge_hl7v2::map::maps::{Condition, Kind};

use crate::support::{self};

#[test]
fn the_package_loads_every_concept_map_of_the_four_kinds() {
    let corpus = support::corpus();
    let count = |kind| corpus.maps().filter(|map| map.kind == kind).count();
    assert_eq!(count(Kind::Message), 13);
    assert_eq!(count(Kind::Segment), 75);
    assert_eq!(count(Kind::Datatype), 105);
    assert_eq!(count(Kind::Table), 70);
    assert_eq!(corpus.maps().count(), 263);
}

#[test]
fn the_corpus_conditions_parse_or_are_refused_as_counted_forms() {
    let corpus = support::corpus();
    let mut computable = 0usize;
    let mut refused = 0usize;
    let mut narrative = 0usize;
    let mut defective = BTreeMap::new();
    let mut unsupported = BTreeMap::new();
    for row in corpus.maps().flat_map(|map| map.rows.iter()) {
        match &row.condition {
            Condition::Computable(_) => computable += 1,
            Condition::Unsupported { text, .. } => {
                refused += 1;
                *unsupported.entry(text.clone()).or_insert(0usize) += 1;
            }
            Condition::Defective { text } => {
                refused += 1;
                *defective.entry(text.clone()).or_insert(0usize) += 1;
            }
            Condition::Narrative => narrative += 1,
            Condition::Always => {}
        }
    }
    let list = |forms: &BTreeMap<String, usize>| {
        forms
            .iter()
            .flat_map(|(text, count)| [format!("  {count} "), text.clone(), String::from("\n")])
            .collect::<String>()
    };
    insta::assert_snapshot!(
        "corpus_conditions",
        format!(
            "computable: {computable}\nrefused: {refused}\nnarrative: {narrative}\n\
             defective (no operand):\n{}unsupported:\n{}",
            list(&defective),
            list(&unsupported)
        )
    );
}

#[test]
fn each_map_names_its_title_and_what_it_maps_from_and_to() {
    let corpus = support::corpus();
    let map = |id: &str| corpus.get(id).expect("a vendored map");
    let segment = map("segment-pid-to-patient");
    assert_eq!(segment.title.as_deref(), Some("Segment PID to Patient Map"));
    assert_eq!(
        segment.ends(),
        [(String::from("pid"), String::from("patient"))]
    );
    assert_eq!(
        map("message-adt-a01-to-bundle").ends(),
        [(String::from("adt-a01"), String::from("bundle"))]
    );
    assert_eq!(
        map("datatype-hd-name-to-messageheader-destination").ends(),
        [(
            String::from("hd"),
            String::from("messageheader-destination")
        )],
        "the qualifier is no part of the source"
    );
    assert_eq!(
        map("table-hl70277-to-v2-0277").ends(),
        [(
            String::from("http://terminology.hl7.org/CodeSystem/v2-0277"),
            String::from("http://terminology.hl7.org/CodeSystem/v2-0277")
        )],
        "a table map names its group's code systems"
    );
    assert!(
        corpus
            .maps()
            .filter(|map| map.kind != Kind::Other)
            .all(|map| !map.ends().is_empty()),
        "every map of the four kinds names what it maps from and to"
    );
}
