// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `Patient` written back as a PID through the guide's maps, with the
//! table maps answered offline.

use ferrobridge_hl7v2::map::inverse::{Outcome, segment};
use ferrobridge_hl7v2::map::single;

use ferrobridge_hl7v2::parse::Delimiters;
use ferrobridge_hl7v2::parse::structure::segment_of;
use ferrobridge_hl7v2::write::Writer;
use fhir_types::codec::Value;

use crate::{fixtures, support};

const DELIMITERS: Delimiters = Delimiters {
    field: '|',
    component: '^',
    repetition: '~',
    escape: '\\',
    subcomponent: '&',
    truncation: None,
};

/// The PID table of HL7 v2.5, as `ADT_A01` places it.
fn pid_2_5() -> &'static hl7v2_types::model::Segment {
    let structure = hl7v2_types::legacy::find("ADT_A01", "2.5").expect("the 2.5 ADT_A01");
    segment_of(structure, "PID").expect("ADT_A01 places a PID")
}

/// Parses a JSON literal into the codec's value.
fn json(text: &str) -> Value {
    Value::from_serde_json(serde_json::from_str(text).expect("a JSON literal"))
}

#[tokio::test]
async fn a_patient_writes_back_as_the_pid_its_fields_came_from() {
    let (_server, terminology) = support::terminology().await;
    let corpus = support::shipped();
    let patient = json(
        r#"{
          "resourceType": "Patient",
          "id": "p-1",
          "extension": [{"url": "http://hl7.org/fhir/StructureDefinition/patient-mothersMaidenName", "valueString": "Roe"}],
          "name": [
            {"use": "official", "family": "Doe", "given": ["Ann", "B"], "prefix": ["Dr"]},
            {"use": "nickname", "family": "Doe", "given": ["Annie"]}
          ],
          "telecom": [
            {"system": "phone", "value": "+31201234567", "use": "home"},
            {"system": "email", "value": "ann@example.org"},
            {"system": "phone", "value": "+31207654321", "use": "work"}
          ],
          "gender": "female",
          "birthDate": "1980-02-29",
          "address": [{"use": "home", "line": ["Main Street 1"], "city": "Springfield", "postalCode": "1234 AB", "country": "NL"}]
        }"#,
    );
    let inverted = segment(&patient, pid_2_5(), corpus, Some(&terminology))
        .await
        .expect("the Patient writes back");
    let written = Writer::new(DELIMITERS).segment(inverted.segment());
    assert_eq!(
        written,
        "PID|||||Doe^Ann^B^^Dr^^L~Doe^Annie^^^^^N|Roe|19800229|F|||Main Street 1^^Springfield^^1234 AB^NL^H||^PRN^PH^^^^^^^^^+31201234567~^^Internet^ann@example.org|^WPN^PH^^^^^^^^^+31207654321",
        "{:?}",
        inverted.outcomes()
    );
}

#[tokio::test]
async fn the_names_write_back_into_pid_5() {
    let (_server, terminology) = support::terminology().await;
    let corpus = support::shipped();
    let patient = json(
        r#"{"resourceType": "Patient", "name": [
            {"use": "official", "family": "Doe", "given": ["Ann", "B"], "prefix": ["Dr"]},
            {"use": "nickname", "family": "Doe", "given": ["Annie"]}
        ]}"#,
    );
    let inverted = segment(&patient, pid_2_5(), corpus, Some(&terminology))
        .await
        .expect("the Patient writes back");
    assert_eq!(
        Writer::new(DELIMITERS).segment(inverted.segment()),
        "PID|||||Doe^Ann^B^^Dr^^L~Doe^Annie^^^^^N",
        "{:?}",
        inverted.outcomes()
    );
}

#[tokio::test]
async fn an_element_no_row_writes_back_is_counted() {
    let (_server, terminology) = support::terminology().await;
    let corpus = support::shipped();
    let patient = json(
        r#"{"resourceType": "Patient", "active": true, "telecom": [{"system": "url", "value": "https://example.org", "use": "temp"}]}"#,
    );
    let inverted = segment(&patient, pid_2_5(), corpus, Some(&terminology))
        .await
        .expect("the Patient writes back");
    let paths: Vec<&str> = inverted
        .outcomes()
        .iter()
        .filter_map(|outcome| match outcome {
            Outcome::Element { path } => Some(path.as_str()),
            _ => None,
        })
        .collect();
    assert!(paths.contains(&"active"), "{:?}", inverted.outcomes());
}

#[tokio::test]
async fn a_code_that_translates_back_to_several_is_ambiguous_never_guessed() {
    let (_server, terminology) = support::terminology().await;
    let guide = support::corpus();
    let patient = json(r#"{"resourceType": "Patient", "gender": "other"}"#);
    let inverted = segment(&patient, pid_2_5(), guide, Some(&terminology))
        .await
        .expect("the Patient writes back");
    assert!(
        inverted.outcomes().iter().any(|outcome| matches!(
            outcome,
            Outcome::Ambiguous { codes, .. } if codes == &["O", "A", "N"]
        )),
        "{:?}",
        inverted.outcomes()
    );
    assert_eq!(Writer::new(DELIMITERS).segment(inverted.segment()), "PID");
    let shipped = support::shipped();
    let preferred = segment(&patient, pid_2_5(), shipped, Some(&terminology))
        .await
        .expect("the Patient writes back");
    assert_eq!(
        Writer::new(DELIMITERS).segment(preferred.segment()),
        "PID||||||||O",
        "the shipped table-administrative-gender-to-hl70001 names O"
    );
}

#[tokio::test]
async fn with_no_translator_a_table_value_is_counted() {
    let corpus = support::shipped();
    let patient = json(r#"{"resourceType": "Patient", "gender": "female"}"#);
    let inverted = segment(&patient, pid_2_5(), corpus, None)
        .await
        .expect("the Patient writes back");
    assert!(
        inverted
            .outcomes()
            .iter()
            .any(|outcome| matches!(outcome, Outcome::NoTranslator { .. })),
        "{:?}",
        inverted.outcomes()
    );
}

#[tokio::test]
async fn a_pid_mapped_forward_writes_back_as_itself() {
    let (_server, terminology) = support::terminology().await;
    let pid = b"PID|1||PAT-1^^^EAST^MR||Doe^Ann^B^^Dr^^L|Roe|19800229|F|||Main Street 1^^Springfield^^1234 AB^NL^H||^PRN^PH^^^^^^^^^+31201234567|^WPN^PH^^^^^^^^^+31207654321";
    let parsed = support::parsed(&fixtures::message(&[
        b"MSH|^~\\&|ADT|EAST|PIX|NORTH|20261025100000||ADT^A01^ADT_A01|M-1|P|2.5",
        b"EVN||20261025100000",
        pid,
        b"PV1|1|O",
    ]));
    let corpus = support::shipped();
    let forward = single::segment(&parsed, 2, "Patient", corpus, Some(&terminology))
        .await
        .expect("the PID maps");
    let patient = &forward.resource().expect("a Patient").resource;
    let inverted = segment(patient, pid_2_5(), corpus, Some(&terminology))
        .await
        .expect("the Patient writes back");
    let written = Writer::new(DELIMITERS).segment(inverted.segment());
    assert_eq!(
        written,
        "PID|||PAT-1^^^^MR||Doe^Ann^B^^Dr^^L|Roe|19800229|F|||Main Street 1^^Springfield^^1234 AB^NL^H||^PRN^PH^^^^^^^^^+31201234567|^WPN^PH^^^^^^^^^+31207654321",
        "{:?}",
        inverted.outcomes()
    );
}

#[test]
fn a_fhir_date_or_time_converts_back_to_its_v2_form() {
    use ferrobridge_hl7v2::map::convert::{ConvertError, temporal};
    assert_eq!(temporal("DTM", "1980-02-29").as_deref(), Ok("19800229"));
    assert_eq!(temporal("DTM", "1980").as_deref(), Ok("1980"));
    assert_eq!(
        temporal("DTM", "2026-10-25T10:00:00.5+01:00").as_deref(),
        Ok("20261025100000.5+0100")
    );
    assert_eq!(
        temporal("TS", "2026-10-25T10:00:00Z").as_deref(),
        Ok("20261025100000+0000")
    );
    assert_eq!(
        temporal("DT", "2026-10-25T10:00:00Z").as_deref(),
        Ok("20261025")
    );
    assert_eq!(temporal("TM", "10:30:00").as_deref(), Ok("103000"));
    assert_eq!(
        temporal("TM", "2026-10-25"),
        Err(ConvertError::Precision { target: "TM" })
    );
    assert!(matches!(
        temporal("DTM", "the 25th"),
        Err(ConvertError::Form { .. })
    ));
}

#[tokio::test]
async fn a_consumer_sets_the_fields_no_row_writes_back() {
    use ferrobridge_hl7v2::write::value::{Component, Field, Repetition};
    let (_server, terminology) = support::terminology().await;
    let corpus = support::shipped();
    let patient =
        json(r#"{"resourceType": "Patient", "name": [{"family": "Doe"}], "gender": "female"}"#);
    let mut pid = segment(&patient, pid_2_5(), corpus, Some(&terminology))
        .await
        .expect("the Patient writes back")
        .into_segment();
    assert_eq!(pid.field(1), Some(&Field::default()));
    assert_eq!(pid.field(9), None);
    pid.set(1, Field::from("1"));
    pid.set(
        3,
        Field::from(Repetition::from_iter([
            Component::from("PAT-1"),
            Component::default(),
            Component::default(),
            Component::from_iter(["EAST", "2.999.1.1", "ISO"]),
        ])),
    );
    pid.set(0, Field::from("ignored"));
    pid.set(10, Field::from("R"));
    assert_eq!(pid.fields().len(), 10);
    assert_eq!(
        Writer::new(DELIMITERS).segment(&pid),
        "PID|1||PAT-1^^^EAST&2.999.1.1&ISO||Doe|||F||R"
    );
}

#[test]
fn the_inverse_and_the_single_map_run_on_a_multi_threaded_runtime() {
    fn spawnable<F: Future + Send>(_: &F) {}
    let corpus = support::shipped();
    // The futures are only built, never polled, so the client reaches no server.
    let terminology = ferrobridge_term::client::Client::new(ferrobridge_term::config::Config::new(
        "http://127.0.0.1:9/r4".parse().expect("a base url"),
        ferrobridge_term::config::WireVersion::R4,
    ))
    .expect("a client");
    let patient = json(r#"{"resourceType": "Patient"}"#);
    spawnable(&segment(&patient, pid_2_5(), corpus, Some(&terminology)));
    let parsed = support::parsed(&fixtures::message(&[
        b"MSH|^~\\&|ADT|EAST|PIX|NORTH|20261025100000||ADT^A01^ADT_A01|M-1|P|2.5",
        b"EVN||20261025100000",
        b"PID|1||PAT-1",
        b"PV1|1|O",
    ]));
    spawnable(&single::segment(
        &parsed,
        2,
        "Patient",
        corpus,
        Some(&terminology),
    ));
}
