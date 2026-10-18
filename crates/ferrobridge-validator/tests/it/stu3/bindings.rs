// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bindings of an STU3 profile: the value set read from
//! `valueSetReference` or `valueSetUri`
//! (<https://hl7.org/fhir/STU3/elementdefinition-definitions.html#ElementDefinition.binding.valueSet_x_>),
//! never decided from the package set, and answered by a terminology server.

use std::time::Duration;

use ferrobridge_term::client::Client;
use ferrobridge_term::config::Config;
use ferrobridge_term::config::RetryPolicy;
use ferrobridge_term::config::WireVersion;
use ferrobridge_testkit::stubs::terminology;
use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use serde_json::json;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::matchers::method;
use wiremock::matchers::path;

use crate::stu3::stu3_profile;
use crate::stu3::stu3_set;
use crate::stu3::validate_stu3;
use crate::support::BASE;
use crate::support::TestResult;
use crate::support::locations;

/// The value set the profile binds the marital status to.
const STATUSES: &str = "http://example.org/fhir/ValueSet/statuses";

/// The code system of the marital status codes.
const SYSTEM: &str = "http://example.org/fhir/CodeSystem/statuses";

/// An STU3 client for `server`, sending each request once.
fn client(server: &MockServer) -> TestResult<Client> {
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

/// Validates a Patient whose marital status is the code `code` of
/// [`SYSTEM`] against a profile binding it with `binding`.
fn report(binding: &serde_json::Value, code: &str) -> TestResult<Report> {
    let definition = stu3_profile(
        "stu3-bound",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.maritalStatus", "path": "Patient.maritalStatus", "min": 0, "max": "1",
             "type": [{"code": "CodeableConcept"}], "binding": binding}
        ]),
    );
    let (_synthetic, set) = stu3_set(&[definition])?;
    let resource = json!({"resourceType": "Patient",
        "maritalStatus": {"coding": [{"system": SYSTEM, "code": code}]}});
    validate_stu3(&set, &resource, Some(&format!("{BASE}stu3-bound")))
}

/// The binding listings of `report` on the marital status.
fn listings(report: &Report) -> Vec<(Constraint, Reason)> {
    report
        .unevaluated()
        .iter()
        .filter(|entry| entry.element == "Patient.maritalStatus")
        .filter(|entry| matches!(entry.constraint, Constraint::Binding { .. }))
        .map(|entry| (entry.constraint.clone(), entry.reason.clone()))
        .collect()
}

/// A required binding to [`STATUSES`] as a `valueSetReference`.
fn by_reference() -> serde_json::Value {
    json!({"strength": "required", "valueSetReference": {"reference": STATUSES}})
}

#[test]
fn a_value_set_reference_binding_is_listed_and_pending_without_a_server() -> TestResult {
    let report = report(&by_reference(), "m")?;
    assert_eq!(
        listings(&report),
        [(
            Constraint::Binding {
                strength: "required".to_owned(),
                value_set: Some(STATUSES.to_owned()),
            },
            Reason::TerminologyNotRead { release: "STU3" },
        )]
    );
    let pending = report.pending_bindings();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].validation().value_set_url, STATUSES);
    Ok(())
}

#[test]
fn a_value_set_uri_binding_reads_as_its_url() -> TestResult {
    let binding = json!({"strength": "extensible", "valueSetUri": STATUSES});
    let report = report(&binding, "m")?;
    assert_eq!(
        listings(&report),
        [(
            Constraint::Binding {
                strength: "extensible".to_owned(),
                value_set: Some(STATUSES.to_owned()),
            },
            Reason::TerminologyNotRead { release: "STU3" },
        )]
    );
    Ok(())
}

#[tokio::test]
async fn a_code_outside_the_value_set_is_an_error_of_the_required_binding() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/stu3/ValueSet/$validate-code"))
        .respond_with(terminology::validate_code(
            false,
            None,
            Some("not in the value set"),
        ))
        .mount(&server)
        .await;
    let mut report = report(&by_reference(), "x")?;
    report.resolve_bindings(&client(&server)?).await;
    assert_eq!(
        locations(&report, MessageId::NotInValueSet),
        ["Patient.maritalStatus"]
    );
    assert!(listings(&report).is_empty(), "{:#?}", report.unevaluated());
    Ok(())
}

#[tokio::test]
async fn a_code_in_the_value_set_lifts_the_listing() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/stu3/ValueSet/$validate-code"))
        .respond_with(terminology::validate_code(true, None, None))
        .mount(&server)
        .await;
    let mut report = report(&by_reference(), "m")?;
    report.resolve_bindings(&client(&server)?).await;
    assert!(report.issues().is_empty(), "{:#?}", report.issues());
    assert!(listings(&report).is_empty(), "{:#?}", report.unevaluated());
    Ok(())
}
