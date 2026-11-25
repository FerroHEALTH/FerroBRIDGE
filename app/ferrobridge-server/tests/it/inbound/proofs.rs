// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A sender's proofs all hold or its request is refused, naming the first
//! proof that failed.

use std::net::IpAddr;

use ferrobridge_server::inbound::verify::{
    Key, Network, Proofs, Refused, Request, StaticHeader, Verifier,
};
use http::{HeaderMap, HeaderValue};

/// Returns the request carrying `headers` with an empty body.
fn request(headers: &HeaderMap) -> Request<'_> {
    Request {
        headers,
        body: b"",
        peer: None,
        now: 0,
    }
}

/// Returns the network verifier over `networks`.
fn network(networks: &[&str]) -> Verifier {
    serde_json::from_value(serde_json::json!({"kind": "network", "networks": networks}))
        .expect("the network verifier reads")
}

/// Returns the static header verifier over `X-Token`.
fn token() -> Verifier {
    Verifier::StaticHeader(StaticHeader {
        header: String::from("X-Token"),
    })
}

/// Returns the one key `value`.
fn key(value: &str) -> Vec<Key> {
    vec![Key::new(value.as_bytes().to_vec())]
}

/// The address of the peer the cases send from.
fn peer() -> Option<IpAddr> {
    "10.20.0.21".parse().ok()
}

#[test]
fn every_proof_holding_admits() {
    let mut headers = HeaderMap::new();
    headers.insert("X-Token", HeaderValue::from_static("secret"));
    let proofs = Proofs::default()
        .with(token(), key("secret"))
        .with(network(&["10.20.0.0/16"]), Vec::new());
    assert_eq!(Ok(()), proofs.verify(&request(&headers), peer()));
}

#[test]
fn one_failing_proof_refuses_whatever_the_others_say() {
    let mut headers = HeaderMap::new();
    headers.insert("X-Token", HeaderValue::from_static("secret"));
    let token = token();
    let outside = network(&["192.0.2.0/24"]);
    let proofs = Proofs::default()
        .with(&token, key("secret"))
        .with(&outside, Vec::new());
    let unproven = proofs
        .verify(&request(&headers), peer())
        .expect_err("the network proof fails")
        .expect("a proof failed");
    assert_eq!(&outside, unproven.proof);
    assert_eq!(Refused::Mismatch, unproven.reason);
}

#[test]
fn the_first_failing_proof_is_named() {
    let headers = HeaderMap::new();
    let token = token();
    let outside = network(&["192.0.2.0/24"]);
    let proofs = Proofs::default()
        .with(&token, key("secret"))
        .with(&outside, Vec::new());
    let unproven = proofs
        .verify(&request(&headers), peer())
        .expect_err("both proofs fail")
        .expect("a proof failed");
    assert_eq!(&token, unproven.proof);
    assert_eq!(Refused::MissingHeader, unproven.reason);
}

#[test]
fn an_empty_set_proves_nothing() {
    let headers = HeaderMap::new();
    assert_eq!(
        Err(None),
        Proofs::default().verify(&request(&headers), peer())
    );
}

#[test]
fn a_network_proof_needs_a_peer_address() {
    let headers = HeaderMap::new();
    let inside = network(&["10.20.0.0/16"]);
    assert_eq!(
        Err(Refused::Mismatch),
        inside.verify(&request(&headers), &[])
    );
    let proofs = Proofs::default().with(&inside, Vec::new());
    assert!(proofs.verify(&request(&headers), None).is_err());
}

#[test]
fn a_network_that_does_not_read_is_refused() {
    let read = serde_json::from_value::<Network>(serde_json::json!({"networks": ["10.0.0.0/33"]}));
    assert!(read.is_err());
}
