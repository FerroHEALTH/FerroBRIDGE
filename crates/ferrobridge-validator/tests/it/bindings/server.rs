// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bindings the package set cannot decide, answered by a `wiremock`
//! terminology server through `ValueSet/$validate-code`
//! (<https://hl7.org/fhir/R4/valueset-operation-validate-code.html>).

use std::time::Duration;

use ferrobridge_term::client::Client;
use ferrobridge_term::config::Config;
use ferrobridge_term::config::RetryPolicy;
use ferrobridge_term::config::WireVersion;
use ferrobridge_term::outcome::Coded;
use ferrobridge_testkit::stubs::terminology;
use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use http::StatusCode;
use serde_json::json;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use crate::bindings::COLORS;
use crate::bindings::binding_listings;
use crate::bindings::bound;
use crate::bindings::check;
use crate::bindings::value_set;
use crate::support::TestResult;
use crate::support::locations;

/// A value set the package set does not carry.
const REMOTE: &str = "http://example.org/fhir/ValueSet/remote";

/// A code system the package set does not carry.
const REMOTE_SYSTEM: &str = "http://example.org/fhir/CodeSystem/remote";

/// A client for `server`, sending each request once.
fn client(server: &MockServer) -> TestResult<Client> {
    let base = format!("{}/r4", server.uri()).parse()?;
    let config = Config::new(base, WireVersion::R4)
        .with_timeout(Duration::from_secs(5))
        .with_retry(RetryPolicy {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
        });
    Ok(Client::new(config)?)
}

/// Mounts `response` on the `$validate-code` route.
async fn mount(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/r4/ValueSet/$validate-code"))
        .respond_with(response)
        .mount(server)
        .await;
}

/// Validates a Patient whose marital status is the coding `coding`
/// against a required binding to [`REMOTE`].
fn remote_report(coding: &serde_json::Value) -> TestResult<Report> {
    let definition = bound(
        "Patient.maritalStatus",
        "CodeableConcept",
        "1",
        &json!({"strength": "required", "valueSet": REMOTE}),
    );
    let resource = json!({"resourceType": "Patient", "maritalStatus": {"coding": [coding]}});
    check(&definition, &[], &resource)
}

/// A coding of [`REMOTE_SYSTEM`].
fn remote(code: &str) -> serde_json::Value {
    json!({"system": REMOTE_SYSTEM, "code": code})
}

#[test]
fn without_a_server_the_binding_is_named_with_its_local_reason() -> TestResult {
    let report = remote_report(&remote("a"))?;
    assert_eq!(
        binding_listings(&report)
            .into_iter()
            .map(|(_, reason)| reason)
            .collect::<Vec<_>>(),
        [Reason::ValueSetNotFound(REMOTE.to_owned())]
    );
    assert_eq!(report.pending_bindings().len(), 1);
    let Coded::CodeableConcept { coding, .. } = &report.pending_bindings()[0].validation().coded
    else {
        panic!("a CodeableConcept goes in its own form");
    };
    assert_eq!(coding.len(), 1);
    Ok(())
}

#[test]
fn a_code_system_the_set_does_not_carry_is_named() -> TestResult {
    let loinc = value_set(
        "loinc",
        &json!({"include": [{"system": "http://loinc.org"}]}),
    );
    let definition = bound(
        "Patient.maritalStatus",
        "CodeableConcept",
        "1",
        &json!({"strength": "required", "valueSet": "http://example.org/fhir/ValueSet/loinc"}),
    );
    let resource = json!({"resourceType": "Patient", "maritalStatus": {"coding": [
        {"system": "http://loinc.org", "code": "1-8"}
    ]}});
    let report = check(&definition, &[loinc], &resource)?;
    assert_eq!(
        binding_listings(&report)
            .into_iter()
            .map(|(_, reason)| reason)
            .collect::<Vec<_>>(),
        [Reason::CodeSystemNotFound("http://loinc.org".to_owned())]
    );
    Ok(())
}

#[test]
fn a_bare_code_with_no_system_to_send_is_named() -> TestResult {
    let definition = bound(
        "Patient.gender",
        "code",
        "1",
        &json!({"strength": "required", "valueSet": REMOTE}),
    );
    let report = check(
        &definition,
        &[],
        &json!({"resourceType": "Patient", "gender": "a"}),
    )?;
    assert_eq!(
        binding_listings(&report)
            .into_iter()
            .map(|(_, reason)| reason)
            .collect::<Vec<_>>(),
        [Reason::NoCodeSystem]
    );
    assert!(report.pending_bindings().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_valid_answer_lifts_the_listing() -> TestResult {
    let server = MockServer::start().await;
    mount(&server, terminology::validate_code(true, None, None)).await;
    let mut report = remote_report(&remote("a"))?;
    report.resolve_bindings(&client(&server)?).await;
    assert!(report.issues().is_empty(), "{:#?}", report.issues());
    assert!(
        binding_listings(&report).is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    assert!(report.pending_bindings().is_empty());
    Ok(())
}

#[tokio::test]
async fn an_invalid_answer_is_an_error_of_the_required_binding() -> TestResult {
    let server = MockServer::start().await;
    mount(
        &server,
        terminology::validate_code(false, None, Some("not in the value set")),
    )
    .await;
    let mut report = remote_report(&remote("b"))?;
    report.resolve_bindings(&client(&server)?).await;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.maritalStatus"]
    );
    assert!(
        binding_listings(&report).is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    Ok(())
}

#[tokio::test]
async fn a_failing_server_leaves_the_binding_named_with_the_upstream_status() -> TestResult {
    let server = MockServer::start().await;
    mount(
        &server,
        terminology::outcome(503, "transient", None, "the server is down"),
    )
    .await;
    let mut report = remote_report(&remote("a"))?;
    report.resolve_bindings(&client(&server)?).await;
    assert!(report.is_valid(), "a failure is never an invalid code");
    assert_eq!(
        binding_listings(&report)
            .into_iter()
            .map(|(_, reason)| reason)
            .collect::<Vec<_>>(),
        [Reason::TerminologyFailed {
            kind: "server-failure",
            status: Some(StatusCode::SERVICE_UNAVAILABLE)
        }]
    );
    Ok(())
}

#[tokio::test]
async fn a_refusal_leaves_the_binding_named_and_never_valid() -> TestResult {
    let server = MockServer::start().await;
    mount(
        &server,
        terminology::outcome(404, "not-found", Some("not-found"), "no such value set"),
    )
    .await;
    let mut report = remote_report(&remote("a"))?;
    report.resolve_bindings(&client(&server)?).await;
    assert_eq!(
        binding_listings(&report)
            .into_iter()
            .map(|(_, reason)| reason)
            .collect::<Vec<_>>(),
        [Reason::TerminologyFailed {
            kind: "refused",
            status: Some(StatusCode::NOT_FOUND)
        }]
    );
    Ok(())
}

#[tokio::test]
async fn several_checks_go_as_one_batch() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/r4"))
        .respond_with(terminology::batch_response(&[
            (
                "200 OK",
                terminology::parameters_resource(&terminology::validate_code_body(
                    true, None, None, None,
                )),
            ),
            (
                "200 OK",
                terminology::parameters_resource(&terminology::validate_code_body(
                    false, None, None, None,
                )),
            ),
        ]))
        .mount(&server)
        .await;
    let definition = bound(
        "Patient.meta.tag",
        "Coding",
        "*",
        &json!({"strength": "required", "valueSet": REMOTE}),
    );
    let resource = json!({"resourceType": "Patient", "meta": {"tag": [
        remote("a"), remote("b"), remote("a")
    ]}});
    let mut report = check(&definition, &[], &resource)?;
    assert_eq!(report.pending_bindings().len(), 3);
    report.resolve_bindings(&client(&server)?).await;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.meta.tag[1]"]
    );
    let received = server.received_requests().await.unwrap_or_default();
    assert_eq!(
        received.len(),
        1,
        "one batch for the two distinct questions"
    );
    let body: serde_json::Value = serde_json::from_slice(&received[0].body)?;
    assert_eq!(body["entry"].as_array().map(Vec::len), Some(2));
    Ok(())
}

#[tokio::test]
async fn a_display_the_server_does_not_give_is_a_warning() -> TestResult {
    let server = MockServer::start().await;
    mount(
        &server,
        terminology::validate_code(true, Some("Alpha"), None),
    )
    .await;
    let definition = bound(
        "Patient.meta.tag",
        "Coding",
        "*",
        &json!({"strength": "required", "valueSet": REMOTE}),
    );
    let mut coding = remote("a");
    coding["display"] = json!("Beta");
    let resource = json!({"resourceType": "Patient", "meta": {"tag": [coding]}});
    let mut report = check(&definition, &[], &resource)?;
    report.resolve_bindings(&client(&server)?).await;
    assert_eq!(
        locations(&report, MessageId::DisplayMismatch),
        ["Patient.meta.tag[0]"]
    );
    assert!(report.is_valid());
    Ok(())
}

#[tokio::test]
async fn a_bare_code_goes_with_the_one_system_its_value_set_names() -> TestResult {
    let server = MockServer::start().await;
    mount(&server, terminology::validate_code(true, None, None)).await;
    let filtered = value_set(
        "filtered",
        &json!({"include": [{"system": COLORS,
                             "filter": [{"property": "hue", "op": "regex", "value": "w.*"}]}]}),
    );
    let definition = bound(
        "Patient.gender",
        "code",
        "1",
        &json!({"strength": "required", "valueSet": "http://example.org/fhir/ValueSet/filtered"}),
    );
    let mut report = check(
        &definition,
        &[filtered],
        &json!({"resourceType": "Patient", "gender": "red"}),
    )?;
    let Coded::Code { system, .. } = &report.pending_bindings()[0].validation().coded else {
        panic!("a bare code goes in the code form");
    };
    assert_eq!(system, COLORS);
    report.resolve_bindings(&client(&server)?).await;
    assert!(
        binding_listings(&report).is_empty(),
        "{:#?}",
        report.unevaluated()
    );
    Ok(())
}
