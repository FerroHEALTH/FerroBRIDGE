// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the face answers each message it is sent, and what it commits.

use super::{CONTROL_ID, FIXTURES, Serving, adt_a01, assert_counted, frame, read_ack, segments};
use super::{Running, face_settings, oru_r01};
use crate::facade::EHR_ID;
use crate::facade::ehr_body;
use crate::facade::settings;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::ehr::Policy;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use ferrobridge_testkit::stubs::its_rest;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

#[tokio::test]
async fn a_mapped_message_commits_one_contribution_and_is_answered_aa() -> Result<(), Box<dyn Error>>
{
    let running = Running::start(true, Duration::ZERO).await?;
    let ack = running.send(&oru_r01(CONTROL_ID)).await?;
    assert!(
        running.serving.listening.is_up(),
        "the listener reads up while serving"
    );
    assert_eq!(
        vec![format!("MSA|AA|{CONTROL_ID}")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    assert!(segments(&ack, "ERR").is_empty(), "{ack}");
    assert!(
        ack.starts_with("MSH|^~\\&|EHR|SOUTHCLINIC|LAB|NORTHLAB|"),
        "the answer swaps the applications: {ack}"
    );
    assert_eq!(1, running.contributions().await, "one commit");
    let committed = running
        .requests("POST", &format!("/ehr/{EHR_ID}/contribution"))
        .await;
    let body = String::from_utf8(committed.first().cloned().unwrap_or_default())?;
    assert!(
        body.contains(CONTROL_ID) && body.contains("ORU^R01"),
        "the FEEDER_AUDIT names the message by MSH-10 and its type"
    );
    assert_counted(&running.serving.usage(), (1, 0, 0), 0);
    running.stop().await
}

#[tokio::test]
async fn a_message_sent_again_is_answered_aa_and_commits_nothing() -> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    let first = running.send(&oru_r01(CONTROL_ID)).await?;
    assert_eq!(
        vec![format!("MSA|AA|{CONTROL_ID}")],
        segments(&first, "MSA"),
        "{first}"
    );
    let again = running.send(&oru_r01(CONTROL_ID)).await?;
    assert_eq!(
        vec![format!("MSA|AA|{CONTROL_ID}")],
        segments(&again, "MSA"),
        "a replay answers as the first delivery did: {again}"
    );
    assert_eq!(
        1,
        running.contributions().await,
        "the replay commits nothing"
    );
    running.stop().await
}

#[tokio::test]
async fn a_message_no_program_maps_is_answered_ae_naming_each_entry() -> Result<(), Box<dyn Error>>
{
    let running = Running::start(false, Duration::ZERO).await?;
    let ack = running.send(&oru_r01(CONTROL_ID)).await?;
    assert_eq!(
        vec![format!("MSA|AE|{CONTROL_ID}")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    let errors = segments(&ack, "ERR");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("carries no entry this server maps")),
        "{ack}"
    );
    assert!(
        errors.iter().any(|error| error.contains("urn:uuid:")
            && error.contains("Observation")
            && error.contains("claims one of the profiles")),
        "the Observation is named with why it was skipped: {ack}"
    );
    assert!(
        errors
            .iter()
            .all(|error| error.contains("|207^Application error^HL70357|E|")),
        "{ack}"
    );
    assert_eq!(0, running.contributions().await, "nothing is committed");
    assert_counted(&running.serving.usage(), (0, 1, 0), 0);
    running.stop().await
}

// HL7 v2 chapter 2 §2.10.3: [FHS] { [BHS] { MSH ... } [BTS] } [FTS]; §2.10.3.2 answers a
// batch with a response batch, and BHS-11 is echoed in BHS-12 of the answering batch.
#[tokio::test]
async fn a_batch_file_is_answered_with_one_acknowledgment_per_message_in_a_response_batch()
-> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    let mut file = b"FHS|^~\\&|LAB|NORTHLAB|EHR|SOUTHCLINIC|20260925143000+0200||||F-1\r".to_vec();
    file.extend_from_slice(b"BHS|^~\\&|LAB|NORTHLAB|EHR|SOUTHCLINIC|20260925143000+0200||||B-1\r");
    file.extend_from_slice(&oru_r01("MSG-V2-B001"));
    file.extend_from_slice(&oru_r01("MSG-V2-B002"));
    file.extend_from_slice(b"BTS|2\rFTS|1\r");
    let answer = running.send(&file).await?;
    let lines: Vec<&str> = answer.split('\r').filter(|line| !line.is_empty()).collect();
    assert!(
        lines.first().is_some_and(|line| {
            line.starts_with("FHS|^~\\&|EHR|SOUTHCLINIC|LAB|NORTHLAB|") && line.ends_with("|F-1")
        }),
        "the file header swaps the parties and references FHS-11: {answer}"
    );
    assert!(
        lines.get(1).is_some_and(|line| {
            line.starts_with("BHS|^~\\&|EHR|SOUTHCLINIC|LAB|NORTHLAB|") && line.ends_with("|B-1")
        }),
        "the batch header references BHS-11: {answer}"
    );
    let acknowledged: Vec<&str> = segments(&answer, "MSA")
        .iter()
        .filter_map(|msa| msa.split('|').nth(2))
        .collect();
    assert_eq!(vec!["MSG-V2-B001", "MSG-V2-B002"], acknowledged, "{answer}");
    assert_eq!(
        lines.iter().rev().take(2).copied().collect::<Vec<_>>(),
        vec!["FTS|1", "BTS|2"],
        "{answer}"
    );
    running.stop().await
}

#[tokio::test]
async fn a_sending_facility_not_listed_is_answered_ar_at_msh_4_and_never_mapped()
-> Result<(), Box<dyn Error>> {
    let mut lane = face_settings(true);
    lane.senders = vec![ferrobridge_server::hl7v2::Sender::Namespace(String::from(
        "SOUTHLAB",
    ))];
    let lookup = ResponseTemplate::new(200).set_body_string(ehr_body());
    let running = Running::start_with(lane, lookup).await?;
    let ack = running.send(&oru_r01(CONTROL_ID)).await?;
    assert_eq!(
        vec![format!("MSA|AR|{CONTROL_ID}")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    let errors = segments(&ack, "ERR");
    assert!(
        errors
            .iter()
            .any(|error| error.starts_with("ERR||MSH^1^4|207^Application error^HL70357|E|")),
        "{ack}"
    );
    assert!(
        running.requests("GET", "/ehr").await.is_empty(),
        "a refused sender's message never reaches the ingest"
    );
    assert_eq!(0, running.contributions().await);
    running.stop().await
}

#[tokio::test]
async fn a_listed_sending_facility_is_mapped() -> Result<(), Box<dyn Error>> {
    let mut lane = face_settings(true);
    lane.senders = vec![ferrobridge_server::hl7v2::Sender::Namespace(String::from(
        "NORTHLAB",
    ))];
    let lookup = ResponseTemplate::new(200).set_body_string(ehr_body());
    let running = Running::start_with(lane, lookup).await?;
    let ack = running.send(&oru_r01(CONTROL_ID)).await?;
    assert_eq!(
        vec![format!("MSA|AA|{CONTROL_ID}")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    assert_eq!(1, running.contributions().await);
    running.stop().await
}

#[tokio::test]
async fn a_cdr_that_fails_is_answered_ar_so_the_sender_resends() -> Result<(), Box<dyn Error>> {
    let running = Running::start_with(face_settings(true), ResponseTemplate::new(503)).await?;
    let ack = running.send(&oru_r01(CONTROL_ID)).await?;
    assert_eq!(
        vec![format!("MSA|AR|{CONTROL_ID}")],
        segments(&ack, "MSA"),
        "a failure unrelated to content is AR (chapter 2 §2.9.2.2): {ack}"
    );
    assert!(!segments(&ack, "ERR").is_empty(), "{ack}");
    assert_eq!(0, running.contributions().await, "nothing is committed");
    assert_counted(&running.serving.usage(), (0, 0, 1), 0);
    running.stop().await
}

#[tokio::test]
async fn a_message_in_flight_when_the_stop_arrives_is_committed_and_answered()
-> Result<(), Box<dyn Error>> {
    let Running { cdr, mut serving } = Running::start(true, Duration::from_millis(500)).await?;
    let mut stream = TcpStream::connect(serving.address).await?;
    stream.write_all(&frame(&oru_r01(CONTROL_ID))).await?;
    let looked_up = || async {
        cdr.received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .any(|request| request.method.as_str() == "GET" && request.url.path() == "/ehr")
    };
    for _ in 0..200 {
        if looked_up().await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        looked_up().await,
        "the message reached the ingest before the stop"
    );
    let listening = serving.listening.clone();
    serving.signal()?;
    let ack = read_ack(&mut stream).await?;
    assert_eq!(
        vec![format!("MSA|AA|{CONTROL_ID}")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    serving.stop().await?;
    let contributions = cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| {
            request.method.as_str() == "POST"
                && request.url.path() == format!("/ehr/{EHR_ID}/contribution")
        })
        .count();
    assert_eq!(1, contributions, "the message in flight committed");
    assert!(!listening.is_up(), "the listener reads down after the stop");
    Ok(())
}

// HL7 v2.5.1 chapter 3 §3.3.1: A01 signals the start of a patient's stay; the face resolves
// the subject's EHR under `[hl7v2] ehr_policy` as the facade does (no specification governs
// the policy: our own design).
#[tokio::test]
async fn an_adt_a01_admits_its_patient_creating_the_ehr_under_the_face_policy()
-> Result<(), Box<dyn Error>> {
    let mut lane = face_settings(true);
    lane.ehr_policy = Some(Policy::CreateOnFirstWrite);
    let running = Running::start_with(lane, its_rest::not_found("no EHR for the subject")).await?;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/ehr"))
        .respond_with(ResponseTemplate::new(201).insert_header("ETag", format!("\"{EHR_ID}\"")))
        .mount(&running.cdr)
        .await;
    let control = "MSG-V2-A01-0001";
    let ack = running.send(&adt_a01(control)).await?;
    assert_eq!(
        vec![format!("MSA|AA|{control}")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    let created = running.requests("POST", "/ehr").await;
    assert_eq!(1, created.len(), "the admission created one EHR");
    let status: serde_json::Value =
        serde_json::from_slice(created.first().ok_or("an EHR_STATUS body")?)?;
    assert_eq!(Some("EHR_STATUS"), status["_type"].as_str(), "{status}");
    assert_eq!(
        Some("synthetic-subject-0001"),
        status["subject"]["external_ref"]["id"]["value"].as_str(),
        "the EHR names the PID-3 subject: {status}"
    );
    assert_eq!(1, running.contributions().await, "one commit into the EHR");
    let committed = running
        .requests("POST", &format!("/ehr/{EHR_ID}/contribution"))
        .await;
    let body = String::from_utf8(committed.first().cloned().unwrap_or_default())?;
    assert!(
        body.contains(control) && body.contains("ADT^A01"),
        "the FEEDER_AUDIT names the admission by MSH-10 and its type"
    );
    assert_counted(&running.serving.usage(), (1, 0, 0), 0);
    running.stop().await
}

#[tokio::test]
async fn an_adt_a01_for_an_unknown_patient_creates_no_ehr_under_the_default_policy()
-> Result<(), Box<dyn Error>> {
    let running = Running::start_with(
        face_settings(true),
        its_rest::not_found("no EHR for the subject"),
    )
    .await?;
    let control = "MSG-V2-A01-0002";
    let ack = running.send(&adt_a01(control)).await?;
    assert_ne!(
        vec![format!("MSA|AA|{control}")],
        segments(&ack, "MSA"),
        "an unknown subject is not accepted under the existing-EHR policy: {ack}"
    );
    assert!(!segments(&ack, "ERR").is_empty(), "{ack}");
    assert!(
        running.requests("POST", "/ehr").await.is_empty(),
        "no EHR is created the policy does not allow"
    );
    assert_eq!(0, running.contributions().await, "nothing is committed");
    running.stop().await
}

#[tokio::test]
async fn an_oru_r01_over_mllp_commits_a_composition_into_a_real_cdr() -> Result<(), Box<dyn Error>>
{
    if !ferrobridge_testkit::containers::e2e_enabled() {
        return Ok(());
    }
    // NOTE: no specification governs this: our own design; no published
    // context maps the guide's resources in-tree (#258), so the suite's own context stands in.
    let cdr = ferrobridge_testkit::containers::cdr().await?;
    crate::facade_e2e::upload_template(cdr.base_url()).await?;
    let client = ferrobridge_server::cdr::CdrClient::new(
        &ferrobridge_server::cdr::config::CdrConfig::new(cdr.base_url().parse()?),
    )?;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let mut facade_settings = settings();
    facade_settings.ehr_policy = Policy::CreateOnFirstWrite;
    let facade = Arc::new(Facade::new(
        compiled,
        Arc::new(MemoryStore::new()),
        client,
        facade_settings,
    ));
    let serving = Serving::start(facade, face_settings(true), None).await?;
    let control = "MSG-V2-E2E-0001";
    let ack = serving.send(&oru_r01(control)).await?;
    assert_eq!(
        vec![format!("MSA|AA|{control}")],
        segments(&ack, "MSA"),
        "the reference CDR committed the message: {ack}"
    );
    let again = serving.send(&oru_r01(control)).await?;
    assert_eq!(
        vec![format!("MSA|AA|{control}")],
        segments(&again, "MSA"),
        "a replay answers as the commit did: {again}"
    );
    serving.stop().await
}

#[tokio::test]
async fn each_map_a_message_ran_through_counts_it_once() -> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    running.send(&oru_r01(CONTROL_ID)).await?;
    running.send(&oru_r01("MSG00002")).await?;
    let document = running.serving.directory.document();
    let messages = |id: &str| {
        document
            .maps
            .iter()
            .find(|map| map.id == id)
            .map(|map| map.messages)
    };
    assert_eq!(Some(2), messages("message-oru-r01-to-bundle"));
    assert_eq!(Some(2), messages("segment-pid-to-patient"));
    assert_eq!(Some(0), messages("message-adt-a01-to-bundle"));
    running.stop().await
}
