// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The clinician a message names as the composer of its compositions (RM EHR
//! IM, `COMPOSITION.composer`, "The person primarily responsible for the
//! content of the Composition"), the refusal of a result that names none, and
//! a device sender's result committed as an act of the device.

use std::error::Error;
use std::time::Duration;

use ferrobridge_server::config::hl7v2::{Device, Sender};
use wiremock::ResponseTemplate;

use super::{CONTROL_ID, Running, adt_a01, face_settings, oru_r01, segments};
use crate::facade::EHR_ID;
use crate::facade::ehr_body;

/// The ordering provider (OBR-16) of the ORU^R01 fixture.
const ORDERING: &str = "|||||||||DR-1^Doctor^Dana^^^^^^NORTHLAB";

/// Returns the ORU^R01 fixture with `edit` applied to its text.
fn edited(control_id: &str, edit: impl Fn(String) -> String) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(edit(String::from_utf8(oru_r01(control_id))?).into_bytes())
}

/// Returns every composition of the one contribution the CDR received.
async fn compositions(running: &Running) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    let committed = running
        .requests("POST", &format!("/ehr/{EHR_ID}/contribution"))
        .await;
    let body: serde_json::Value =
        serde_json::from_slice(committed.first().ok_or("one contribution")?)?;
    Ok(body["versions"]
        .as_array()
        .ok_or("the versions")?
        .iter()
        .map(|version| version["data"].clone())
        .collect())
}

#[tokio::test]
async fn the_ordering_provider_is_the_composer() -> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    running.send(&oru_r01(CONTROL_ID)).await?;
    let compositions = compositions(&running).await?;
    assert!(!compositions.is_empty(), "one composition");
    for composition in &compositions {
        let composer = &composition["composer"];
        assert_eq!(Some("Dana Doctor"), composer["name"].as_str(), "{composer}");
        assert_eq!(
            Some("DR-1"),
            composer
                .pointer("/external_ref/id/value")
                .and_then(serde_json::Value::as_str),
            "{composer}"
        );
        assert_eq!(
            Some("NORTHLAB"),
            composer
                .pointer("/external_ref/namespace")
                .and_then(serde_json::Value::as_str),
            "XCN.9 is the namespace: {composer}"
        );
    }
    running.stop().await
}

#[tokio::test]
async fn the_responsible_observer_wins_over_the_ordering_provider() -> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    running
        .send(&edited("MSG-CLIN-OBX16", |text| {
            text.replace(
                "|||20260925130000+0200",
                "|||20260925130000+0200||OBS-7^Observer^Omar^^^^^^NORTHLAB",
            )
        })?)
        .await?;
    for composition in compositions(&running).await? {
        assert_eq!(
            Some("Omar Observer"),
            composition["composer"]["name"].as_str(),
            "OBX-16 is read before ORC-12 and OBR-16: {composition}"
        );
    }
    running.stop().await
}

#[tokio::test]
async fn the_event_operator_is_the_composer_of_an_admission() -> Result<(), Box<dyn Error>> {
    let lookup = ResponseTemplate::new(200).set_body_string(ehr_body());
    let running = Running::start_with(face_settings(true), lookup).await?;
    let ack = running.send(&adt_a01("MSG-CLIN-EVN5")).await?;
    assert_eq!(
        vec![String::from("MSA|AA|MSG-CLIN-EVN5")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    let compositions = compositions(&running).await?;
    assert!(!compositions.is_empty(), "one composition");
    for composition in &compositions {
        let composer = &composition["composer"];
        assert_eq!(
            Some("Olive Operator"),
            composer["name"].as_str(),
            "EVN-5 is read first: {composer}"
        );
        assert_eq!(
            Some("OP-1"),
            composer
                .pointer("/external_ref/id/value")
                .and_then(serde_json::Value::as_str),
            "{composer}"
        );
    }
    running.stop().await
}

#[tokio::test]
async fn a_result_naming_no_clinician_is_answered_ae() -> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    let ack = running
        .send(&edited("MSG-CLIN-NONE", |text| text.replace(ORDERING, ""))?)
        .await?;
    assert_eq!(
        vec![String::from("MSA|AE|MSG-CLIN-NONE")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    assert!(
        segments(&ack, "ERR")
            .iter()
            .any(|error| error.contains("OBX^1^16") && error.contains("clinician")),
        "the ERR names the missing clinician: {ack}"
    );
    assert_eq!(0, running.contributions().await, "nothing is committed");
    running.stop().await
}

#[tokio::test]
async fn a_device_sender_s_result_commits_as_a_device_act() -> Result<(), Box<dyn Error>> {
    let mut lane = face_settings(true);
    lane.devices = vec![Device {
        sender: Sender::Namespace(String::from("NORTHLAB")),
        identifier_issuer: String::from("urn:example:devices"),
    }];
    let lookup = ResponseTemplate::new(200).set_body_string(ehr_body());
    let running = Running::start_with(lane, lookup).await?;
    let ack = running
        .send(&edited("MSG-CLIN-DEVICE", |text| {
            text.replace(ORDERING, "").replace(
                "|||20260925130000+0200",
                "|||20260925130000+0200||||ANALYSER-9",
            )
        })?)
        .await?;
    assert_eq!(
        vec![String::from("MSA|AA|MSG-CLIN-DEVICE")],
        segments(&ack, "MSA"),
        "a device's result needs no clinician: {ack}"
    );
    for composition in compositions(&running).await? {
        let composer = &composition["composer"];
        assert_eq!(
            Some("ANALYSER-9"),
            composer
                .pointer("/external_ref/id/value")
                .and_then(serde_json::Value::as_str),
            "OBX-18 names the device: {composer}"
        );
        assert_eq!(
            Some("urn:example:devices"),
            composer
                .pointer("/external_ref/namespace")
                .and_then(serde_json::Value::as_str),
            "{composer}"
        );
        assert_eq!(
            Some("ANALYSER-9"),
            composition
                .pointer("/feeder_audit/originating_system_audit/system_id")
                .and_then(serde_json::Value::as_str),
            "the device is the originating system: {composition}"
        );
    }
    running.stop().await
}
