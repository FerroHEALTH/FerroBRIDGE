// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! MLLP over TLS: a peer with a certificate from the configured authorities
//! is served and its certificate reaches the handler, and a peer without one
//! never completes the handshake. The certificate path is RFC 5280's as
//! `rustls` validates it; requiring one is our own design.

use std::sync::Arc;
use std::time::Duration;

use ferrobridge_hl7v2::mllp::Codec;
use ferrobridge_hl7v2::mllp::listener::{Connection, Handler, Timeouts, serve_tls};
use ferrobridge_hl7v2::mllp::tls;
use ferrobridge_testkit::mllp_tls::{Pki, connect};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// The DNS name the case's sender certificate is issued for.
const SENDER: &str = "lab.example.org";

/// A handler that answers whether the peer's certificate names [`SENDER`].
struct Named;

impl Handler for Named {
    fn handle(
        &self,
        _message: Vec<u8>,
        connection: &Connection,
    ) -> impl Future<Output = Option<Vec<u8>>> + Send {
        let named = connection
            .certificate()
            .is_some_and(|certificate| tls::names(certificate, SENDER));
        std::future::ready(Some(if named {
            b"named".to_vec()
        } else {
            b"stranger".to_vec()
        }))
    }
}

/// Wraps `message` in the MLLP envelope.
fn frame(message: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x0B];
    bytes.extend_from_slice(message);
    bytes.extend_from_slice(&[0x1C, 0x0D]);
    bytes
}

/// Starts a TLS listener over `pki` and returns its address and stop switch.
async fn start(pki: &Pki) -> (std::net::SocketAddr, oneshot::Sender<()>) {
    let acceptor = tls::acceptor(
        pki.server_certificate().as_bytes(),
        pki.server_key().as_bytes(),
        pki.ca().as_bytes(),
    )
    .expect("the acceptor builds");
    let socket = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a local port");
    let address = socket.local_addr().expect("an address");
    let (stop, stopped) = oneshot::channel::<()>();
    tokio::spawn(serve_tls(
        socket,
        acceptor,
        Arc::new(Named),
        Codec::new(1024),
        Timeouts::default(),
        async {
            let _signal = stopped.await;
        },
    ));
    (address, stop)
}

#[tokio::test]
async fn a_peer_with_a_certificate_is_served_and_named() {
    let pki = Pki::generate().expect("the material");
    let (address, _stop) = start(&pki).await;
    let identity = pki.client(SENDER).expect("a client certificate");
    let connector = pki.connector(Some(&identity)).expect("a connector");
    let mut stream = connect(address, &connector).await.expect("the handshake");
    stream
        .write_all(&frame(b"MSH|^~\\&|"))
        .await
        .expect("the frame is sent");
    let mut answer = vec![0u8; 8];
    let read = tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut answer))
        .await
        .expect("an answer in time")
        .expect("the read succeeds");
    assert_eq!(b"\x0bnamed\x1c\x0d"[..], answer[..read]);
}

#[tokio::test]
async fn a_peer_with_another_name_is_served_as_a_stranger() {
    let pki = Pki::generate().expect("the material");
    let (address, _stop) = start(&pki).await;
    let identity = pki
        .client("other.example.org")
        .expect("a client certificate");
    let connector = pki.connector(Some(&identity)).expect("a connector");
    let mut stream = connect(address, &connector).await.expect("the handshake");
    stream.write_all(&frame(b"MSH|")).await.expect("sent");
    let mut answer = vec![0u8; 11];
    let read = tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut answer))
        .await
        .expect("an answer in time")
        .expect("the read succeeds");
    assert_eq!(b"\x0bstranger\x1c\x0d"[..], answer[..read]);
}

#[tokio::test]
async fn a_peer_without_a_certificate_is_never_served() {
    let pki = Pki::generate().expect("the material");
    let (address, _stop) = start(&pki).await;
    let connector = pki.connector(None).expect("a connector");
    // NOTE: RFC 8446 §4.4.2.4: a server that requires a client certificate aborts
    // the handshake; under TLS 1.3 the client may learn it only on its first read.
    let refused = match connect(address, &connector).await {
        Err(_) => true,
        Ok(mut stream) => {
            let _sent = stream.write_all(&frame(b"MSH|")).await;
            let mut answer = vec![0u8; 8];
            !matches!(
                tokio::time::timeout(Duration::from_secs(5), stream.read(&mut answer)).await,
                Ok(Ok(read)) if read > 0
            )
        }
    };
    assert!(refused, "a peer without a certificate got an answer");
}

#[tokio::test]
async fn an_acceptor_without_authorities_is_refused() {
    let pki = Pki::generate().expect("the material");
    let refused = tls::acceptor(
        pki.server_certificate().as_bytes(),
        pki.server_key().as_bytes(),
        b"",
    );
    assert!(matches!(refused, Err(tls::TlsError::NoAuthority)));
}
