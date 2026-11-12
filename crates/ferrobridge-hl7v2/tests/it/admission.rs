// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The handler's two connection decisions on a plain listener: a peer it
//! does not admit is closed before any byte is read, and a connection it
//! asks to close ends right after the answer. No specification governs
//! either: our own design.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ferrobridge_hl7v2::mllp::Codec;
use ferrobridge_hl7v2::mllp::listener::{Connection, Handler, Timeouts, serve_with};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

use crate::fixtures;
use crate::support::Face;

/// A handler that admits no peer.
#[derive(Debug, Default)]
struct Nobody;

impl Handler for Nobody {
    // A peer that is not admitted never reaches the handler, so these bytes never
    // reach it either.
    fn handle(
        &self,
        _message: Vec<u8>,
        _connection: &Connection,
    ) -> impl Future<Output = Option<Vec<u8>>> + Send {
        std::future::ready(Some(b"handled".to_vec()))
    }

    fn admits(&self, _peer: SocketAddr) -> bool {
        false
    }
}

/// A handler that answers as the wire tests' face does and then asks for
/// the connection to close.
#[derive(Debug, Default)]
struct Once;

impl Handler for Once {
    async fn handle(&self, message: Vec<u8>, connection: &Connection) -> Option<Vec<u8>> {
        connection.close();
        Face.handle(message, connection).await
    }
}

/// Serves `handler` on an ephemeral port, sends one framed ADT^A01, and
/// returns every byte the listener wrote before it closed the connection.
async fn exchange<H: Handler>(handler: H) -> Vec<u8> {
    let socket = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let address = socket.local_addr().expect("an address");
    let (stop, stopped) = oneshot::channel::<()>();
    let task = tokio::spawn(serve_with(
        socket,
        Arc::new(handler),
        Codec::new(1024 * 1024),
        Timeouts::default(),
        async {
            let _signal = stopped.await;
        },
    ));
    let mut stream = TcpStream::connect(address)
        .await
        .expect("the listener accepts");
    let mut bytes = vec![0x0B];
    bytes.extend_from_slice(&fixtures::adt_a01());
    bytes.extend_from_slice(&[0x1C, 0x0D]);
    // NOTE: no specification governs this: our own design; a refused peer may see
    // its write reset, which is the closed connection the case asserts.
    let _sent = stream.write_all(&bytes).await;
    let mut received = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut received))
        .await
        .expect("the listener closes the connection in time");
    if let Err(error) = read {
        assert_eq!(
            std::io::ErrorKind::ConnectionReset,
            error.kind(),
            "a closed connection, not another failure"
        );
    }
    stop.send(()).expect("the listener is running");
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("the listener stops")
        .expect("the task joins")
        .expect("the listener ends cleanly");
    received
}

#[tokio::test]
async fn a_peer_the_handler_does_not_admit_is_closed_unanswered() {
    let received = exchange(Nobody).await;
    assert!(received.is_empty(), "nothing is written to a refused peer");
}

#[tokio::test]
async fn a_connection_the_handler_closes_ends_after_its_answer() {
    let received = exchange(Once).await;
    let text = String::from_utf8_lossy(&received);
    assert!(text.starts_with('\u{b}'), "one framed answer: {text}");
    assert!(
        text.ends_with("\u{1c}\r"),
        "the frame ends the stream: {text}"
    );
    assert!(text.contains("MSA|AA|MSG00002"), "{text}");
}
