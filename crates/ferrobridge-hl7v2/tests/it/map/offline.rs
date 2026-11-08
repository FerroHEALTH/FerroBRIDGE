// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The offline translator over the guide's table maps, against the same maps
//! answered by a terminology server.

use ferrobridge_hl7v2::map::map;
use ferrobridge_hl7v2::map::translate::Translator;
use ferrobridge_term::outcome::TranslateOutcome;
use ferrobridge_term::request::Translation;

use crate::fixtures;
use crate::map::{mapped, summary};
use crate::support;

const GENDER_MAP: &str =
    "http://hl7.org/fhir/uv/v2mappings/ConceptMap/table-hl70001-to-administrative-gender";
const TABLE_0001: &str = "http://terminology.hl7.org/CodeSystem/v2-0001";
const GENDER: &str = "http://hl7.org/fhir/administrative-gender";

/// The accepted codes an outcome names, each with its system.
fn accepted(outcome: &TranslateOutcome) -> Vec<(String, String)> {
    outcome
        .accepted()
        .filter_map(|found| found.concept.as_ref())
        .filter_map(|concept| Some((concept.system.clone()?, concept.code.clone()?)))
        .collect()
}

#[tokio::test]
async fn a_table_code_translates_through_the_map_its_url_names() {
    let corpus = support::corpus();
    let translated = Translator::Offline(&corpus)
        .translate(&Translation::new(TABLE_0001, "F").with_concept_map(GENDER_MAP))
        .await
        .expect("the offline translator never fails");
    assert_eq!(
        accepted(&translated),
        vec![(String::from(GENDER), String::from("female"))]
    );
}

#[tokio::test]
async fn a_reverse_translation_returns_every_code_mapped_to_the_target() {
    let corpus = support::corpus();
    let translated = Translator::Offline(&corpus)
        .translate(
            &Translation::new(GENDER, "other")
                .with_concept_map(GENDER_MAP)
                .with_reverse(true),
        )
        .await
        .expect("the offline translator never fails");
    let codes: Vec<String> = accepted(&translated)
        .into_iter()
        .map(|(system, code)| {
            assert_eq!(system, TABLE_0001);
            code
        })
        .collect();
    assert_eq!(codes, ["O", "A", "N"]);
}

#[tokio::test]
async fn a_code_no_map_holds_and_a_map_the_corpus_lacks_are_no_match() {
    let corpus = support::corpus();
    let offline = Translator::Offline(&corpus);
    let unknown = offline
        .translate(&Translation::new(TABLE_0001, "Q").with_concept_map(GENDER_MAP))
        .await
        .expect("the offline translator never fails");
    assert!(matches!(unknown, TranslateOutcome::NoMatch { .. }));
    let elsewhere = offline
        .translate(&Translation::new(TABLE_0001, "F").with_concept_map("http://example.org/none"))
        .await
        .expect("the offline translator never fails");
    assert!(matches!(elsewhere, TranslateOutcome::NoMatch { .. }));
}

#[tokio::test]
async fn an_unmatched_target_is_no_translation() {
    let corpus = support::corpus();
    let translated = Translator::Offline(&corpus)
        .translate(
            &Translation::new("http://terminology.hl7.org/CodeSystem/v2-0190", "M")
                .with_concept_map(
                    "http://hl7.org/fhir/uv/v2mappings/ConceptMap/table-hl70190-to-address-use",
                ),
        )
        .await
        .expect("the offline translator never fails");
    assert!(matches!(translated, TranslateOutcome::NoMatch { .. }));
}

#[tokio::test]
async fn the_offline_tables_map_a_message_as_the_server_does() {
    for fixture in [fixtures::adt_a01(), fixtures::oru_r01()] {
        let (served, _) = mapped(&fixture).await;
        let parsed = support::parsed(&fixture);
        let corpus = support::corpus();
        let offline = map(&parsed, &corpus, Some(Translator::Offline(&corpus)))
            .await
            .expect("the message maps");
        assert_eq!(summary(&offline), summary(&served));
        assert_eq!(offline.translations(), served.translations());
        assert_eq!(codes(offline.bundle()), codes(served.bundle()));
    }
}

/// Every `code` and `system` string in a document, in document order.
fn codes(value: &fhir_types::codec::Value) -> Vec<String> {
    let mut found = Vec::new();
    walk(value, &mut found);
    found
}

fn walk(value: &fhir_types::codec::Value, found: &mut Vec<String>) {
    match value {
        fhir_types::codec::Value::Object(object) => {
            for (key, inner) in object {
                if (key == "code" || key == "system" || key == "gender" || key == "status")
                    && let Some(text) = inner.as_str()
                {
                    found.push(format!("{key}={text}"));
                }
                walk(inner, found);
            }
        }
        fhir_types::codec::Value::Array(items) => {
            for item in items {
                walk(item, found);
            }
        }
        _ => {}
    }
}
