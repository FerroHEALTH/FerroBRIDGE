// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bindings a terminology server answers when the client speaks STU3.
//! STU3 `ValueSet/$validate-code` declares `url`, `coding` and
//! `codeableConcept` as R4 does, and no `valueSetVersion`
//! (<https://hl7.org/fhir/STU3/valueset-operations.html#validate-code>), so a
//! binding to a versioned value set cannot be asked and stays listed with the
//! client's refusal while the other questions still go.

use std::time::Duration;

use ferrobridge_term::client::Client;
use ferrobridge_term::config::Config;
use ferrobridge_term::config::RetryPolicy;
use ferrobridge_term::config::WireVersion;
use ferrobridge_testkit::stubs::terminology;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use serde_json::json;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::matchers::method;
use wiremock::matchers::path;

use crate::bindings::binding_listings;
use crate::bindings::bound_url;
use crate::bindings::terminology_set;
use crate::support::TestResult;
use crate::support::profile;
use crate::support::validate;

/// A value set the package set does not carry.
const REMOTE: &str = "http://example.org/fhir/ValueSet/remote";

/// A code system the package set does not carry.
const REMOTE_SYSTEM: &str = "http://example.org/fhir/CodeSystem/remote";

/// An STU3 client for `server`, sending each request once.
fn stu3_client(server: &MockServer) -> TestResult<Client> {
    let base = format!("{}/stu3", server.uri()).parse()?;
    let config = Config::new(base, WireVersion::Stu3)
        .with_timeout(Duration::from_secs(5))
        .with_retry(RetryPolicy {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
        });
    Ok(Client::new(config)?)
}

/// Validates a Patient whose marital status and one tag carry codes of
/// [`REMOTE_SYSTEM`], bound required to `status` and `tag`.
fn report(status: &str, tag: &str) -> TestResult<Report> {
    let definition = profile(
        "bound",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.meta", "path": "Patient.meta", "min": 0, "max": "1"},
            {"id": "Patient.meta.tag", "path": "Patient.meta.tag", "min": 0, "max": "*",
             "type": [{"code": "Coding"}], "binding": {"strength": "required", "valueSet": tag}},
            {"id": "Patient.maritalStatus", "path": "Patient.maritalStatus", "min": 0, "max": "1",
             "type": [{"code": "CodeableConcept"}],
             "binding": {"strength": "required", "valueSet": status}}
        ]),
    );
    let (_synthetic, set) = terminology_set(&[definition])?;
    let resource = json!({"resourceType": "Patient",
        "meta": {"tag": [{"system": REMOTE_SYSTEM, "code": "t"}]},
        "maritalStatus": {"coding": [{"system": REMOTE_SYSTEM, "code": "s"}]}});
    validate(&set, &resource, Some(&bound_url()))
}

#[tokio::test]
async fn an_stu3_server_answers_a_binding_to_an_unversioned_value_set() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/stu3/ValueSet/$validate-code"))
        .respond_with(terminology::validate_code(true, None, None))
        .mount(&server)
        .await;
    let mut report = report(&format!("{REMOTE}|2.0"), REMOTE)?;
    assert_eq!(report.pending_bindings().len(), 2);
    report.resolve_bindings(&stu3_client(&server)?).await;
    let received = server.received_requests().await.unwrap_or_default();
    assert_eq!(
        received.len(),
        1,
        "one call for the one question STU3 carries"
    );
    let body: serde_json::Value = serde_json::from_slice(&received[0].body)?;
    let names: Vec<&str> = body["parameter"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|parameter| parameter["name"].as_str())
        .collect();
    assert!(
        names.contains(&"url") && names.contains(&"coding"),
        "{names:?}"
    );
    assert!(!names.contains(&"valueSetVersion"), "{names:?}");
    let listed = binding_listings(&report);
    assert_eq!(listed.len(), 1, "{:#?}", report.unevaluated());
    assert_eq!(
        listed[0].1,
        Reason::TerminologyFailed {
            kind: "unsupported-parameter",
            status: None
        }
    );
    Ok(())
}

#[tokio::test]
async fn an_stu3_client_sends_nothing_for_a_versioned_binding_alone() -> TestResult {
    let server = MockServer::start().await;
    let versioned = format!("{REMOTE}|2.0");
    let mut report = report(&versioned, &versioned)?;
    report.resolve_bindings(&stu3_client(&server)?).await;
    let received = server.received_requests().await.unwrap_or_default();
    assert!(received.is_empty(), "nothing is sent");
    let listed = binding_listings(&report);
    assert_eq!(listed.len(), 2, "{:#?}", report.unevaluated());
    assert!(listed.iter().all(|(_, reason)| *reason
        == Reason::TerminologyFailed {
            kind: "unsupported-parameter",
            status: None
        }));
    Ok(())
}
