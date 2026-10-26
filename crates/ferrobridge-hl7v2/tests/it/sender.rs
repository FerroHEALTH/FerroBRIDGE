// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The MLLP sender against the crate's own listener and against peers that
//! misbehave (MLLP Release 1 block format; HL7 v2.5.1 chapter 2 §2.9.2.2 for
//! the acknowledgment codes).

use std::sync::Arc;
use std::time::Duration;

use ferrobridge_hl7v2::mllp::{Codec, SendError, SendTimeouts, send, serve};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::fixtures;
use crate::support::Face;

/// Timeouts long enough for any local exchange.
const TIMEOUTS: SendTimeouts = SendTimeouts {
    connect: Duration::from_secs(5),
    reply: Duration::from_secs(5),
};

/// Starts a peer that reads one frame's bytes and then answers `reply` raw,
/// or holds the connection open when `hold` is set, and returns its address.
async fn peer(reply: &'static [u8], hold: bool) -> std::net::SocketAddr {
    let socket = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let address = socket.local_addr().expect("an address");
    tokio::spawn(async move {
        let (mut stream, _) = socket.accept().await.expect("a connection");
        let mut read = vec![0u8; 64 * 1024];
        let mut received = Vec::new();
        while !received.ends_with(&[0x1C, 0x0D]) {
            let count = stream.read(&mut read).await.expect("the read succeeds");
            if count == 0 {
                return;
            }
            received.extend_from_slice(&read[..count]);
        }
        stream.write_all(reply).await.expect("the reply is sent");
        if hold {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
    address
}

#[tokio::test]
async fn a_message_sent_to_the_listener_comes_back_acknowledged_aa() {
    let socket = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let address = socket.local_addr().expect("an address");
    let (stop, stopped) = oneshot::channel::<()>();
    let task = tokio::spawn(serve(socket, Arc::new(Face), Codec::default(), async {
        let _signal = stopped.await;
    }));
    let reply = send(address, &fixtures::adt_a01(), Codec::default(), TIMEOUTS)
        .await
        .expect("a reply");
    let text = String::from_utf8(reply).expect("an ASCII acknowledgment");
    let msa = text
        .split('\r')
        .find(|segment| segment.starts_with("MSA|"))
        .expect("an MSA segment");
    assert_eq!(msa, "MSA|AA|MSG00002");
    stop.send(()).expect("the listener is running");
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("the listener stops")
        .expect("the task joins")
        .expect("the listener ends cleanly");
}

#[tokio::test]
async fn a_peer_that_listens_on_nothing_refuses_the_connection() {
    let socket = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let address = socket.local_addr().expect("an address");
    drop(socket);
    let sent = send(address, b"MSH|^~\\&|A\r", Codec::default(), TIMEOUTS).await;
    assert!(matches!(sent, Err(SendError::Refused(_))), "{sent:?}");
}

#[tokio::test(start_paused = true)]
async fn a_connection_not_open_within_its_timeout_is_reported_so() {
    // NOTE: RFC 5737 §3 reserves 192.0.2.0/24 for documentation, so no peer
    // answers the handshake, and the paused clock runs out the timeout at once.
    let timeouts = SendTimeouts {
        connect: Duration::from_secs(5),
        ..TIMEOUTS
    };
    let sent = send(
        "192.0.2.1:2575",
        b"MSH|^~\\&|A\r",
        Codec::default(),
        timeouts,
    )
    .await;
    assert!(
        matches!(sent, Err(SendError::ConnectTimedOut { limit }) if limit == Duration::from_secs(5)),
        "{sent:?}"
    );
}

#[tokio::test]
async fn a_peer_that_closes_without_replying_is_reported_closed() {
    let address = peer(b"", false).await;
    let sent = send(address, b"MSH|^~\\&|A\r", Codec::default(), TIMEOUTS).await;
    assert!(matches!(sent, Err(SendError::Closed)), "{sent:?}");
}

#[tokio::test]
async fn a_peer_that_closes_inside_its_reply_is_reported_truncated() {
    let address = peer(b"\x0bMSH|^~\\&|B", false).await;
    let sent = send(address, b"MSH|^~\\&|A\r", Codec::default(), TIMEOUTS).await;
    assert!(matches!(sent, Err(SendError::Truncated)), "{sent:?}");
}

#[tokio::test]
async fn a_reply_past_the_ceiling_is_refused() {
    let address = peer(b"\x0bMSH|^~\\&|B|C|D|E|F|G|H\x1c\x0d", true).await;
    let sent = send(address, b"MSH|^~\\&|A\r", Codec::new(8), TIMEOUTS).await;
    assert!(
        matches!(sent, Err(SendError::TooLarge { limit: 8 })),
        "{sent:?}"
    );
}

#[tokio::test]
async fn a_reply_that_is_no_frame_is_refused() {
    let address = peer(b"MSH|^~\\&|B\x1c\x0d", true).await;
    let sent = send(address, b"MSH|^~\\&|A\r", Codec::default(), TIMEOUTS).await;
    assert!(matches!(sent, Err(SendError::Malformed { .. })), "{sent:?}");
}

#[tokio::test]
async fn a_peer_that_never_replies_times_the_exchange_out() {
    let address = peer(b"", true).await;
    let timeouts = SendTimeouts {
        reply: Duration::from_millis(200),
        ..TIMEOUTS
    };
    let sent = send(address, b"MSH|^~\\&|A\r", Codec::default(), timeouts).await;
    assert!(
        matches!(sent, Err(SendError::ReplyTimedOut { limit }) if limit == Duration::from_millis(200)),
        "{sent:?}"
    );
}

#[tokio::test]
async fn a_message_holding_a_block_character_is_refused_before_connecting() {
    let sent = send("127.0.0.1:9", b"MSH|\x1c", Codec::default(), TIMEOUTS).await;
    assert!(matches!(sent, Err(SendError::Unframable)), "{sent:?}");
}
