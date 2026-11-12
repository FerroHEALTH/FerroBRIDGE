// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The plain listener for the devices that cannot speak TLS: a peer outside
//! every plain sender's networks is closed before any byte is read, and a
//! message whose MSH-4 names no plain sender of the peer's network is
//! answered `AR` and its connection closed. HL7 MLLP Release 2 defines no
//! security layer; tying a plain sender to its networks is our own design.

use std::error::Error;
use std::time::Duration;

use ferrobridge_server::config::Hl7v2Settings;
use ferrobridge_server::config::hl7v2::{Device, Plain, PlainSender, Sender};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use wiremock::ResponseTemplate;

use super::{Running, face_settings, frame, oru_r01, read_ack, segments};
use crate::facade::ehr_body;

/// The ordering provider (OBR-16) of the ORU^R01 fixture.
const ORDERING: &str = "|||||||||DR-1^Doctor^Dana^^^^^^NORTHLAB";

/// Returns a plain device sender of `facility` connecting from `networks`.
fn device(facility: &str, networks: &[&str]) -> Result<PlainSender, Box<dyn Error>> {
    Ok(PlainSender {
        sender: Sender::Namespace(facility.to_owned()),
        sources: networks
            .iter()
            .map(|network| network.parse())
            .collect::<Result<Vec<ipnet::IpNet>, _>>()?,
    })
}

/// Returns the face's settings with `plain` as its plain senders, each a
/// listed sender and a device, on a plain listener.
fn plain_settings(plain: Vec<PlainSender>) -> Result<Hl7v2Settings, Box<dyn Error>> {
    let mut lane = face_settings(true);
    for sender in &plain {
        lane.senders.push(sender.sender.clone());
        lane.devices.push(Device {
            sender: sender.sender.clone(),
            identifier_issuer: String::from("urn:example:devices"),
        });
    }
    lane.plain = Some(Plain {
        bind: "127.0.0.1:0".parse()?,
        senders: plain,
    });
    Ok(lane)
}

/// Returns a device's ORU^R01 sent as `facility`: no ordering provider, and
/// OBX-18 naming the analyser.
fn result_of(facility: &str, control_id: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let text = String::from_utf8(oru_r01(control_id))?
        .replacen("|LAB|NORTHLAB|", &format!("|LAB|{facility}|"), 1)
        .replace(ORDERING, "")
        .replace(
            "|||20260925130000+0200",
            "|||20260925130000+0200||||ANALYSER-9",
        );
    Ok(text.into_bytes())
}

/// Starts the face on its plain listener with `plain` as its plain senders.
async fn start(plain: Vec<PlainSender>) -> Result<Running, Box<dyn Error>> {
    let lookup = ResponseTemplate::new(200).set_body_string(ehr_body());
    Running::start_with(plain_settings(plain)?, lookup).await
}

/// Reads until the listener closes `stream`, returning every byte it wrote;
/// a reset counts as the close it is.
async fn rest(stream: &mut TcpStream) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut received = Vec::new();
    match tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut received)).await? {
        Ok(_) => Ok(received),
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => Ok(received),
        Err(error) => Err(error.into()),
    }
}

#[tokio::test]
async fn a_device_on_its_network_commits_over_plain_mllp() -> Result<(), Box<dyn Error>> {
    let running = start(vec![device("ANALYSER", &["127.0.0.1/32"])?]).await?;
    let mut stream = TcpStream::connect(running.serving.address).await?;
    stream
        .write_all(&frame(&result_of("ANALYSER", "MSG-PLAIN-1")?))
        .await?;
    let ack = read_ack(&mut stream).await?;
    assert_eq!(
        vec!["MSA|AA|MSG-PLAIN-1"],
        segments(&ack, "MSA"),
        "a device on its network is taken: {ack}"
    );
    assert_eq!(1, running.contributions().await, "the message is committed");
    running.stop().await
}

#[tokio::test]
async fn a_peer_outside_every_plain_network_is_closed_unanswered() -> Result<(), Box<dyn Error>> {
    let running = start(vec![device("ANALYSER", &["10.20.0.21/32"])?]).await?;
    let mut stream = TcpStream::connect(running.serving.address).await?;
    let _sent = stream
        .write_all(&frame(&result_of("ANALYSER", "MSG-PLAIN-2")?))
        .await;
    let received = rest(&mut stream).await?;
    assert!(
        received.is_empty(),
        "no acknowledgment reaches a peer outside the networks"
    );
    assert_eq!(0, running.contributions().await, "nothing is committed");
    running.stop().await
}

#[tokio::test]
async fn a_facility_of_another_network_is_answered_ar_and_closed() -> Result<(), Box<dyn Error>> {
    let running = start(vec![
        device("ANALYSER", &["10.20.0.21/32"])?,
        device("BENCHTOP", &["127.0.0.0/8"])?,
    ])
    .await?;
    let mut stream = TcpStream::connect(running.serving.address).await?;
    stream
        .write_all(&frame(&result_of("ANALYSER", "MSG-PLAIN-3")?))
        .await?;
    let ack = read_ack(&mut stream).await?;
    assert_eq!(vec!["MSA|AR|MSG-PLAIN-3"], segments(&ack, "MSA"), "{ack}");
    assert!(
        segments(&ack, "ERR")
            .iter()
            .any(|error| error.contains("MSH^1^4") && error.contains("network address")),
        "{ack}"
    );
    assert!(
        rest(&mut stream).await?.is_empty(),
        "the connection closes after the AR"
    );
    assert_eq!(0, running.contributions().await, "nothing is committed");
    running.stop().await
}

#[tokio::test]
async fn a_tls_sender_s_facility_on_the_plain_listener_is_answered_ar() -> Result<(), Box<dyn Error>>
{
    let running = start(vec![device("ANALYSER", &["127.0.0.1/32"])?]).await?;
    let mut stream = TcpStream::connect(running.serving.address).await?;
    stream
        .write_all(&frame(&result_of("NORTHLAB", "MSG-PLAIN-4")?))
        .await?;
    let ack = read_ack(&mut stream).await?;
    assert_eq!(
        vec!["MSA|AR|MSG-PLAIN-4"],
        segments(&ack, "MSA"),
        "NORTHLAB sends over TLS alone: {ack}"
    );
    assert!(
        rest(&mut stream).await?.is_empty(),
        "the connection closes after the AR"
    );
    assert_eq!(0, running.contributions().await, "nothing is committed");
    running.stop().await
}

#[tokio::test]
async fn a_plain_device_s_identifier_is_still_its_issuer_s() -> Result<(), Box<dyn Error>> {
    let running = start(vec![device("ANALYSER", &["127.0.0.1/32"])?]).await?;
    let mut stream = TcpStream::connect(running.serving.address).await?;
    stream
        .write_all(&frame(&result_of("ANALYSER", "MSG-PLAIN-5")?))
        .await?;
    let ack = read_ack(&mut stream).await?;
    assert_eq!(vec!["MSA|AA|MSG-PLAIN-5"], segments(&ack, "MSA"), "{ack}");
    let committed = running
        .requests(
            "POST",
            &format!("/ehr/{}/contribution", crate::facade::EHR_ID),
        )
        .await;
    let body: serde_json::Value =
        serde_json::from_slice(committed.first().ok_or("one contribution")?)?;
    let composer = body
        .pointer("/versions/0/data/composer/external_ref")
        .ok_or("a composer")?;
    assert_eq!(
        Some("urn:example:devices"),
        composer
            .pointer("/namespace")
            .and_then(serde_json::Value::as_str),
        "OBX-18 names the device in its sender's issuer: {composer}"
    );
    assert_eq!(
        Some("ANALYSER-9"),
        composer
            .pointer("/id/value")
            .and_then(serde_json::Value::as_str),
        "{composer}"
    );
    running.stop().await
}
