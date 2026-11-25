// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Senders routed from the state store: a device added through the operator
//! API is accepted by every replica on its next message, and a rotated
//! client authorities file is in force for the next handshake without a
//! restart. The routing store is our own design; the acknowledgments are HL7
//! v2 original mode's.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrobridge_operator_api::sources::{Kind, Write};
use ferrobridge_server::config::hl7v2::Tls;
use ferrobridge_server::sources::Sources;
use ferrobridge_server::store::Store;
use ferrobridge_testkit::containers;
use ferrobridge_testkit::mllp_tls::Pki;
use ferrobridge_testkit::post_quantum::Peer;
use tokio::io::AsyncWriteExt;
use wiremock::ResponseTemplate;

use super::{
    CONTROL_ID, NORTHLAB_SUBJECT, PKI, PROFILE, Running, face_settings, frame, listeners, oru_r01,
    read_ack, route, segments,
};
use crate::facade::ehr_body;

/// A replica's sources over the database `postgres` holds, listening.
async fn replica(postgres: &containers::Postgres) -> Result<Arc<Sources>, Box<dyn Error>> {
    let store = Store::open(&ferrobridge_server::store::Settings {
        connection: omop_cdm::connection::CdmConnection::new(postgres.url(), None)?,
        max_connections: 4,
        acquire_timeout: Duration::from_secs(30),
        seal: None,
    })
    .await?;
    let sources = Arc::new(Sources::open(&store, None)?);
    store.listeners(&[sources.listened()]).spawn();
    assert!(
        sources
            .listened()
            .listening_within(Duration::from_secs(10))
            .await,
        "the listener connects"
    );
    Ok(sources)
}

#[tokio::test]
async fn a_device_added_in_one_call_is_accepted_by_both_replicas() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let (one, two) = (replica(&postgres).await?, replica(&postgres).await?);
    let lookup = || ResponseTemplate::new(200).set_body_string(ehr_body());
    let first = Running::start_over(
        (listeners(), Arc::<Sources>::clone(&one)),
        lookup(),
        None,
        ferrobridge_testkit::fixtures::DIAGNOSE_OPT,
    )
    .await?;
    let second = Running::start_over(
        (listeners(), Arc::<Sources>::clone(&two)),
        lookup(),
        None,
        ferrobridge_testkit::fixtures::DIAGNOSE_OPT,
    )
    .await?;
    let before = first.send(&oru_r01("MSG-ROUTE-0")).await?;
    assert_eq!(
        vec!["MSA|AR|MSG-ROUTE-0"],
        segments(&before, "MSA"),
        "{before}"
    );

    let seen = (
        one.listened().notifications(),
        two.listened().notifications(),
    );
    let write = Write {
        name: String::from("northlab"),
        kind: Kind::Mllp,
        enabled: true,
        document: serde_json::json!({
            "routes": [route("LAB", "NORTHLAB", Some(NORTHLAB_SUBJECT), &[])],
            "profiles": { "Observation": PROFILE },
        }),
    };
    let document = one.validate(write.kind, &write.document)?;
    one.store()
        .create(&write, &document, "operator-test")
        .await?;
    assert!(
        one.listened()
            .notified_after(seen.0, Duration::from_secs(10))
            .await
    );
    assert!(
        two.listened()
            .notified_after(seen.1, Duration::from_secs(10))
            .await
    );

    for (running, control) in [(&first, "MSG-ROUTE-1"), (&second, "MSG-ROUTE-2")] {
        let ack = running.send(&oru_r01(control)).await?;
        assert_eq!(
            vec![format!("MSA|AA|{control}")],
            segments(&ack, "MSA"),
            "{ack}"
        );
    }
    first.stop().await?;
    second.stop().await
}

#[tokio::test]
async fn a_route_that_disagrees_with_the_client_certificate_is_answered_ar()
-> Result<(), Box<dyn Error>> {
    let mut lane = face_settings(true);
    lane.source()["routes"] =
        serde_json::json!([route("LAB", "NORTHLAB", Some("southlab.example.org"), &[])]);
    let running =
        Running::start_with(lane, ResponseTemplate::new(200).set_body_string(ehr_body())).await?;
    let ack = running.send(&oru_r01(CONTROL_ID)).await?;
    assert_eq!(
        vec![format!("MSA|AR|{CONTROL_ID}")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    assert!(
        segments(&ack, "ERR")
            .iter()
            .any(|error| error.contains("client certificate")),
        "{ack}"
    );
    assert_eq!(0, running.contributions().await, "nothing is committed");
    running.stop().await
}

#[tokio::test]
async fn a_rotated_client_authorities_file_is_in_force_without_a_restart()
-> Result<(), Box<dyn Error>> {
    let rotated = Pki::generate()?;
    let directory = tempfile::tempdir()?;
    let authorities = directory.path().join("authorities.pem");
    std::fs::write(&authorities, PKI.ca())?;
    let mut lane = face_settings(true);
    lane.listeners.tls = Some(Tls {
        acceptor: ferrobridge_server::hl7v2::authorities::acceptor(
            PKI.server_certificate().as_bytes(),
            PKI.server_key().as_bytes(),
            &authorities,
        )?,
    });
    let running =
        Running::start_with(lane, ResponseTemplate::new(200).set_body_string(ehr_body())).await?;
    let address = running.serving.address;
    let old = PKI.client(NORTHLAB_SUBJECT)?;
    let new = rotated.client(NORTHLAB_SUBJECT)?;

    let send = async |identity, control: &str| -> Result<Option<String>, Box<dyn Error>> {
        let connector = PKI.connector(Some(identity), Peer::Hybrid)?;
        let Ok(mut stream) = ferrobridge_testkit::mllp_tls::connect(address, &connector).await
        else {
            return Ok(None);
        };
        if stream.write_all(&frame(&oru_r01(control))).await.is_err() {
            return Ok(None);
        }
        Ok(read_ack(&mut stream).await.ok())
    };
    let accepted = send(&old, "MSG-CA-1")
        .await?
        .ok_or("the first authority admits")?;
    assert_eq!(
        vec!["MSA|AA|MSG-CA-1"],
        segments(&accepted, "MSA"),
        "{accepted}"
    );
    assert!(
        send(&new, "MSG-CA-2").await?.is_none(),
        "an unknown authority is refused"
    );

    std::fs::write(&authorities, rotated.ca())?;
    let file = std::fs::File::options().write(true).open(&authorities)?;
    let written = file.metadata()?.modified()?;
    file.set_modified(written + Duration::from_secs(10))?;
    let accepted = send(&new, "MSG-CA-3")
        .await?
        .ok_or("the rotated authority admits")?;
    assert_eq!(
        vec!["MSA|AA|MSG-CA-3"],
        segments(&accepted, "MSA"),
        "{accepted}"
    );
    assert!(
        send(&old, "MSG-CA-4").await?.is_none(),
        "the replaced authority is refused"
    );
    running.stop().await
}
