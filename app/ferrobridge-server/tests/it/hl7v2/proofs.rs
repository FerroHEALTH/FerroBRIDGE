// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A route's proofs all hold or the message is refused: a route bound to a
//! certificate subject and to networks needs both on the TLS listener, and
//! its certificate proof can never hold on the plain one. Binding a route to
//! its connection is our own design.

use std::net::SocketAddr;

use ferrobridge_hl7v2::mllp::listener::Connection;
use ferrobridge_server::hl7v2::answer::Settled;
use ferrobridge_server::hl7v2::plain::{Lane, unproven};
use ferrobridge_server::sources::document::Route;
use rustls::pki_types::CertificateDer;

/// The subject the sender's client certificate names.
const SUBJECT: &str = "northlab.example.org";

/// Returns the route of `LAB^NORTHLAB` bound to `subject` and `networks`.
fn route(subject: Option<&str>, networks: &[&str]) -> Route {
    serde_json::from_value(serde_json::json!({
        "sending_application": "LAB",
        "sending_facility": "NORTHLAB",
        "certificate_subject": subject,
        "source_cidrs": networks,
    }))
    .expect("the route reads")
}

/// Returns a self-signed certificate naming `subject`.
fn certificate(subject: &str) -> CertificateDer<'static> {
    let key = rcgen::KeyPair::generate().expect("a key");
    rcgen::CertificateParams::new(vec![String::from(subject)])
        .expect("parameters")
        .self_signed(&key)
        .expect("a certificate")
        .der()
        .clone()
}

/// Returns a connection from `peer` that presented `certificate`.
fn connection(peer: &str, certificate: Option<CertificateDer<'static>>) -> Connection {
    let peer: SocketAddr = peer.parse().expect("a socket address");
    Connection::with_peer(Some(peer)).with_certificate(certificate)
}

#[test]
fn a_route_with_both_proofs_takes_a_tls_sender_that_proves_both() {
    let route = route(Some(SUBJECT), &["10.20.0.0/16"]);
    let sender = connection("10.20.0.21:4000", Some(certificate(SUBJECT)));
    assert!(unproven(Lane::Tls, &route, &sender).is_none());
}

#[test]
fn a_tls_sender_outside_its_route_s_networks_is_refused_by_address() {
    let route = route(Some(SUBJECT), &["10.20.0.0/16"]);
    let sender = connection("192.0.2.7:4000", Some(certificate(SUBJECT)));
    assert!(matches!(
        unproven(Lane::Tls, &route, &sender),
        Some(Settled::Address)
    ));
}

#[test]
fn a_tls_sender_on_its_network_with_another_certificate_is_refused_by_certificate() {
    let route = route(Some(SUBJECT), &["10.20.0.0/16"]);
    let sender = connection("10.20.0.21:4000", Some(certificate("southlab.example.org")));
    assert!(matches!(
        unproven(Lane::Tls, &route, &sender),
        Some(Settled::Certificate)
    ));
}

#[test]
fn a_route_with_both_proofs_refuses_the_plain_listener() {
    let route = route(Some(SUBJECT), &["10.20.0.0/16"]);
    let device = connection("10.20.0.21:4000", None);
    assert!(matches!(
        unproven(Lane::Plain, &route, &device),
        Some(Settled::Certificate)
    ));
}

#[test]
fn a_route_bound_to_networks_alone_is_refused_on_the_tls_listener() {
    let route = route(None, &["10.20.0.0/16"]);
    let sender = connection("10.20.0.21:4000", Some(certificate(SUBJECT)));
    assert!(matches!(
        unproven(Lane::Tls, &route, &sender),
        Some(Settled::Certificate)
    ));
}

#[test]
fn an_ipv4_device_seen_as_a_mapped_ipv6_peer_lies_in_its_network() {
    let route = route(None, &["10.20.0.0/16"]);
    let device = connection("[::ffff:10.20.0.21]:4000", None);
    assert!(unproven(Lane::Plain, &route, &device).is_none());
}
