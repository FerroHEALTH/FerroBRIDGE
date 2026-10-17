// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bridge client against a stub bridge: the typed answer, and every way
//! a call fails as a typed error that keeps the bridge's status.

use std::time::Duration;

use http::StatusCode;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use ferrobridge_viewer::bridge::{BODY_LIMIT, Bridge, BridgeError, FHIR_JSON, REFUSAL_LIMIT};
use ferrobridge_viewer::config::settings::BridgeSettings;

/// A synthetic R4 `CapabilityStatement` with the elements R4 requires.
const STATEMENT: &str = r#"{
  "resourceType": "CapabilityStatement",
  "status": "active",
  "date": "2026-10-17",
  "kind": "instance",
  "fhirVersion": "4.0.1",
  "format": ["json"]
}"#;

fn client(server: &MockServer, root: &str) -> Bridge {
    let base = format!("{}{root}", server.uri()).parse().expect("a URL");
    Bridge::new(&BridgeSettings {
        base,
        timeout: Duration::from_secs(5),
    })
    .expect("the client builds")
}

async fn answering(template: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/bridge/fhir/metadata"))
        .and(header("accept", FHIR_JSON))
        .respond_with(template)
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn the_capability_statement_is_read_under_the_base_path() {
    let server =
        answering(ResponseTemplate::new(200).set_body_raw(STATEMENT, "application/fhir+json"))
            .await;
    let statement = client(&server, "/bridge/")
        .capability_statement()
        .await
        .expect("the statement decodes");
    assert_eq!(Some("4.0.1"), statement.fhir_version.value.as_deref());
    assert_eq!(Some("instance"), statement.kind.value.as_deref());
}

#[tokio::test]
async fn a_refusal_keeps_the_bridge_status_and_body() {
    let outcome = r#"{"resourceType":"OperationOutcome","issue":[]}"#;
    let server = answering(ResponseTemplate::new(401).set_body_string(outcome)).await;
    let error = client(&server, "/bridge/")
        .capability_statement()
        .await
        .expect_err("a refusal");
    let BridgeError::Refused { status, body } = error else {
        panic!("a refusal, got {error:?}");
    };
    assert_eq!(StatusCode::UNAUTHORIZED, status);
    assert_eq!(outcome, body);
}

#[tokio::test]
async fn a_refusal_body_is_cut_at_its_limit() {
    let long = "x".repeat(REFUSAL_LIMIT + 100);
    let server = answering(ResponseTemplate::new(503).set_body_string(long)).await;
    let error = client(&server, "/bridge/")
        .capability_statement()
        .await
        .expect_err("a refusal");
    let BridgeError::Refused { status, body } = error else {
        panic!("a refusal, got {error:?}");
    };
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status);
    assert_eq!(REFUSAL_LIMIT, body.len());
}

#[tokio::test]
async fn an_accepted_body_over_the_limit_is_refused() {
    let big = vec![b' '; BODY_LIMIT + 1];
    let server = answering(ResponseTemplate::new(200).set_body_bytes(big)).await;
    let error = client(&server, "/bridge/")
        .capability_statement()
        .await
        .expect_err("too large");
    assert!(matches!(error, BridgeError::TooLarge { .. }), "{error:?}");
}

#[tokio::test]
async fn a_body_that_is_not_the_resource_is_a_typed_error() {
    for (body, shape) in [
        ("not json", "payload"),
        ("[1]", "not an object"),
        (
            r#"{"resourceType":"CapabilityStatement","status":7}"#,
            "decode",
        ),
    ] {
        let server = answering(ResponseTemplate::new(200).set_body_string(body)).await;
        let error = client(&server, "/bridge/")
            .capability_statement()
            .await
            .expect_err("no statement");
        let matched = match shape {
            "payload" => matches!(error, BridgeError::Payload { .. }),
            "not an object" => matches!(error, BridgeError::NotAnObject),
            _ => matches!(error, BridgeError::Decode { .. }),
        };
        assert!(matched, "{body}: {error:?}");
    }
}

#[tokio::test]
async fn a_bridge_that_does_not_answer_is_a_transport_error() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let address = listener.local_addr().expect("its address");
    drop(listener);
    let bridge = Bridge::new(&BridgeSettings {
        base: format!("http://{address}/").parse().expect("a URL"),
        timeout: Duration::from_secs(5),
    })
    .expect("the client builds");
    let error = bridge
        .capability_statement()
        .await
        .expect_err("nothing listens");
    assert!(matches!(error, BridgeError::Transport { .. }), "{error:?}");
}
