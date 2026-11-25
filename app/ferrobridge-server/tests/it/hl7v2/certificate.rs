// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The MLLP face over TLS: a sender sends only on a connection whose client
//! certificate names the subject it is declared under, and a peer with no
//! client certificate is never served. The key exchange prefers X25519MLKEM768
//! (RFC 10024) and still serves a classical sender. Tying a sender to its certificate is
//! our own design.

use std::error::Error;
use std::time::Duration;

use ferrobridge_testkit::post_quantum::{Peer, agreed};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{CONTROL_ID, PKI, Running, frame, oru_r01, read_ack, segments};

#[tokio::test]
async fn a_sender_on_another_sender_s_certificate_is_answered_ar() -> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    let stranger = PKI.client("southlab.example.org")?;
    let connector = PKI.connector(Some(&stranger), Peer::Hybrid)?;
    let mut stream =
        ferrobridge_testkit::mllp_tls::connect(running.serving.address, &connector).await?;
    stream.write_all(&frame(&oru_r01(CONTROL_ID))).await?;
    let ack = read_ack(&mut stream).await?;
    assert_eq!(
        vec![format!("MSA|AR|{CONTROL_ID}")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    assert!(
        segments(&ack, "ERR")
            .iter()
            .any(|error| error.contains("MSH^1^4") && error.contains("client certificate")),
        "{ack}"
    );
    assert_eq!(0, running.contributions().await, "nothing is committed");
    running.stop().await
}

#[tokio::test]
async fn a_peer_without_a_client_certificate_is_never_served() -> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    let connector = PKI.connector(None, Peer::Hybrid)?;
    // NOTE: RFC 8446 §4.4.2.4: the server aborts a handshake that brings no
    // certificate it requires; under TLS 1.3 the client learns it on its read.
    let served =
        match ferrobridge_testkit::mllp_tls::connect(running.serving.address, &connector).await {
            Err(_) => false,
            Ok(mut stream) => {
                let _sent = stream.write_all(&frame(&oru_r01(CONTROL_ID))).await;
                let mut answer = vec![0u8; 8];
                matches!(
                    tokio::time::timeout(Duration::from_secs(5), stream.read(&mut answer)).await,
                    Ok(Ok(read)) if read > 0
                )
            }
        };
    assert!(!served, "a peer without a certificate got an answer");
    assert_eq!(0, running.contributions().await, "nothing is committed");
    running.stop().await
}

// NOTE: RFC 10024; the face agrees on X25519MLKEM768 with a sender that offers
// it, and on a classical group with one that does not.
#[tokio::test]
async fn the_face_agrees_on_the_hybrid_group_and_still_serves_a_classical_sender()
-> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    for peer in Peer::BOTH {
        let connector = PKI.connector(Some(&super::NORTHLAB), peer)?;
        let stream =
            ferrobridge_testkit::mllp_tls::connect(running.serving.address, &connector).await?;
        assert_eq!(Some(peer.agreed()), agreed(stream.get_ref().1), "{peer:?}");
    }
    running.stop().await
}
