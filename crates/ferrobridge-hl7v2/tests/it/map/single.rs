// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One PID mapped to one `Patient` through `segment-pid-to-patient`, with
//! the table maps answered offline.

use ferrobridge_hl7v2::map::single::{SegmentError, segment};

use ferrobridge_hl7v2::map::{MapError, Outcome};
use fhir_types::codec::{Json as _, Path, Value};
use fhir_types::r4::patient::Patient;

use crate::{fixtures, support};

/// A synthetic admission whose PID carries the fields an ITI-8 feed sends.
fn feed() -> Vec<u8> {
    fixtures::message(&[
        b"MSH|^~\\&|ADT|EAST|PIX|NORTH|20261025100000||ADT^A01^ADT_A01|M-1|P|2.5",
        b"EVN||20261025100000",
        b"PID|1||PAT-1^^^EAST^MR||Doe^Ann^B^^Dr^^L~Doe^Annie^^^^^N|Roe|19800229|F|||Main Street 1^^Springfield^^1234 AB^NL^H||^PRN^PH^^^^^^^^^+31201234567~^NET^Internet^ann@example.org|^WPN^PH^^^^^^^^^+31207654321",
        b"PV1|1|O",
    ])
}

/// The index of the PID in a parsed fixture.
const PID: usize = 2;

#[tokio::test]
async fn a_pid_maps_to_one_patient_through_the_guide_s_maps() {
    let (_server, terminology) = support::terminology().await;
    let parsed = support::parsed(&feed());
    let corpus = support::shipped();
    let mapped = segment(&parsed, PID, "Patient", corpus, Some(&terminology))
        .await
        .expect("the PID maps");
    let built = mapped.resource().expect("a Patient");
    assert_eq!(built.resource_type, "Patient");
    let Value::Object(object) = &built.resource else {
        panic!("a JSON object");
    };
    let patient = Patient::from_json(object, &mut Path::root("Patient")).expect("an R4 Patient");
    assert_eq!(
        patient.gender.and_then(|gender| gender.value).as_deref(),
        Some("female")
    );
    assert_eq!(
        patient.birth_date.and_then(|date| date.value).as_deref(),
        Some("1980-02-29")
    );
    assert_eq!(patient.name.len(), 2, "both PID-5 repetitions");
    let first = patient.name.first().expect("a name");
    assert_eq!(
        first
            .family
            .as_ref()
            .and_then(|family| family.value.as_deref()),
        Some("Doe")
    );
    assert_eq!(
        first.r#use.as_ref().and_then(|code| code.value.as_deref()),
        Some("official"),
        "table 0200 L"
    );
    assert_eq!(patient.address.len(), 1);
    let lines: Vec<Option<&str>> = patient.address[0]
        .line
        .iter()
        .map(|line| line.value.as_deref())
        .collect();
    assert_eq!(
        lines,
        [Some("Main Street 1")],
        "the shipped datatype-xad-to-address supplement runs SAD into line"
    );
    assert_eq!(
        patient.address[0]
            .r#use
            .as_ref()
            .and_then(|code| code.value.as_deref()),
        Some("home"),
        "table 0190 H"
    );
    assert_eq!(patient.telecom.len(), 3, "two from PID-13, one from PID-14");
    assert_eq!(patient.identifier.len(), 1, "PID-3");
    assert_eq!(
        patient.extension.len(),
        1,
        "PID-6, the mother's maiden name"
    );
    assert_eq!(
        built.resource.get("extension"),
        Some(&serde_json_value(
            r#"[{"url":"http://hl7.org/fhir/StructureDefinition/patient-mothersMaidenName","valueString":"Roe"}]"#
        )),
        "the shipped datatype-xpn-to-string supplement writes PID-6"
    );
    assert!(mapped.translations() > 0);
    let untranslated: Vec<&str> = mapped
        .outcomes()
        .iter()
        .filter_map(|outcome| match outcome {
            Outcome::Untranslated { row, .. } => Some(row.source.as_str()),
            Outcome::NoTerminology { .. } => Some("no-terminology"),
            _ => None,
        })
        .collect();
    assert_eq!(
        untranslated,
        ["XTN.2"],
        "NET, which table 0201 maps to no use"
    );
    assert!(
        !mapped
            .outcomes()
            .iter()
            .any(|outcome| matches!(outcome, Outcome::MissingRequired { resource, .. } if resource == "Bundle")),
        "the envelope of a single segment is never built"
    );
}

#[tokio::test]
async fn the_guide_alone_leaves_the_mother_s_maiden_name_unwritten() {
    let (_server, terminology) = support::terminology().await;
    let parsed = support::parsed(&feed());
    let corpus = support::corpus();
    let mapped = segment(&parsed, PID, "Patient", corpus, Some(&terminology))
        .await
        .expect("the PID maps");
    assert!(
        mapped.outcomes().iter().any(|outcome| matches!(
            outcome,
            Outcome::UnsupportedTarget { row, .. } if row.map == "datatype-xpn-to-string" && row.target == "$this"
        )),
        "{:?}",
        mapped.outcomes()
    );
}

/// Parses a JSON literal into the codec's value.
fn serde_json_value(text: &str) -> Value {
    Value::from_serde_json(serde_json::from_str(text).expect("a JSON literal"))
}

#[tokio::test]
async fn with_no_translator_a_table_value_is_counted_never_passed_through() {
    let parsed = support::parsed(&feed());
    let corpus = support::corpus();
    let mapped = segment(&parsed, PID, "Patient", corpus, None)
        .await
        .expect("the PID maps");
    assert!(
        mapped
            .outcomes()
            .iter()
            .any(|outcome| matches!(outcome, Outcome::NoTerminology { .. })),
        "{:?}",
        mapped.outcomes()
    );
    let Some(built) = mapped.resource() else {
        panic!("a Patient");
    };
    assert!(built.resource.get("gender").is_none(), "no code is guessed");
}

#[tokio::test]
async fn a_segment_with_no_map_to_the_resource_is_refused() {
    let parsed = support::parsed(&feed());
    let corpus = support::corpus();
    let refused = segment(&parsed, PID, "Observation", corpus, None).await;
    assert!(
        matches!(&refused, Err(SegmentError::NoSegmentMap { segment, .. }) if segment == "PID"),
        "{refused:?}"
    );
}

#[tokio::test]
async fn an_index_with_no_placed_segment_is_refused() {
    let parsed = support::parsed(&feed());
    let corpus = support::corpus();
    let refused = segment(&parsed, 9, "Patient", corpus, None).await;
    assert!(
        matches!(refused, Err(SegmentError::Unplaced { index: 9 })),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_refused_translation_fails_the_segment_with_the_upstream_status() {
    let parsed = support::parsed(&feed());
    let corpus = support::corpus();
    let server = wiremock::MockServer::start().await;
    let base = url::Url::parse(&format!("{}/", server.uri())).expect("a url");
    let config =
        ferrobridge_term::config::Config::new(base, ferrobridge_term::config::WireVersion::R4);
    let client = ferrobridge_term::client::Client::new(config).expect("a client");
    let refused = segment(&parsed, PID, "Patient", corpus, Some(&client)).await;
    assert!(
        matches!(
            refused,
            Err(SegmentError::Map(MapError::Terminology { .. }))
        ),
        "{refused:?}"
    );
}
