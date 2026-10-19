// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The transform playground and the validation dry run against a stub
//! bridge: `$tofhir` and `$toopenehr` with the payload in the request body
//! alone, `$validate` on a type the `CapabilityStatement` declares it on, a
//! refusal and an operations lane that is off drawn with their status, the
//! input the console does not send, the plain post it refuses, and no
//! payload in any log line.

use std::error::Error;

use axum::body::Body;
use ferrobridge_testkit::oidc::{Provider, TokenAnswer};
use http::{Request, StatusCode};
use serde_json::json;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{Captured, OPERATOR_TOKEN, configuration, console, get_with, send, sign_in};

/// A marker in every payload, which no log line may carry.
const MARKER: &str = "synthetic-payload-marker";

/// A synthetic canonical composition carrying [`MARKER`].
fn composition() -> String {
    json!({"_type": "COMPOSITION", "name": {"value": MARKER}}).to_string()
}

/// A synthetic `Bundle` carrying [`MARKER`].
fn bundle() -> String {
    json!({"resourceType": "Bundle", "type": "collection", "entry": [{"resource": {"resourceType": "Observation", "id": "obs-1", "status": "final", "code": {"text": MARKER}}}]}).to_string()
}

/// The console over `bridge` and a signed-in session's cookie.
async fn signed_in(bridge: &MockServer) -> Result<(axum::Router, String), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let base = format!("{}/", bridge.uri());
    let (_state, service) = console(&configuration(&provider, &base, "", ""))?;
    let cookie = sign_in(&service, &provider, TokenAnswer::bearer(OPERATOR_TOKEN)).await?;
    Ok((service, cookie))
}

/// The form-urlencoded body of `pairs`.
fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

/// The HTML the tool at `route` draws for `pairs`, sent as the page's
/// bundle sends it.
async fn ran(
    service: &axum::Router,
    route: &str,
    pairs: &[(&str, &str)],
    cookie: &str,
) -> Result<String, Box<dyn Error>> {
    let request = Request::post(route)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", cookie)
        .body(Body::from(form(pairs)))?;
    let (response, body) = send(service, request).await?;
    assert_eq!(StatusCode::OK, response.status(), "{route}: {body}");
    assert_eq!(
        Some("no-store"),
        response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        "a tool's answer is never stored"
    );
    Ok(serde_json::from_str::<String>(&body)?)
}

/// Asks `$tofhir` with [`composition`], a template and a patient.
async fn to_fhir(service: &axum::Router, cookie: &str) -> Result<String, Box<dyn Error>> {
    let composition = composition();
    ran(
        service,
        "/api/tofhir",
        &[
            ("composition", composition.as_str()),
            ("template_id", "synthetic.vital_signs.v1"),
            ("patient", "Patient/synthetic-1"),
        ],
        cookie,
    )
    .await
}

#[tokio::test]
async fn tofhir_sends_the_payload_in_the_body_and_draws_each_resource() -> Result<(), Box<dyn Error>>
{
    let bridge = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/$tofhir"))
        .and(body_string_contains("\"name\":\"composition\""))
        .and(body_string_contains(
            "\"reference\":\"Patient/synthetic-1\"",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(bundle(), "application/fhir+json"))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let html = to_fhir(&service, &cookie).await?;
    assert!(html.contains("<code>Observation/obs-1</code>"), "{html}");
    assert!(html.contains("The Bundle holds 1 resource."), "{html}");
    assert!(html.contains("The Bundle as JSON"), "{html}");
    let asked = bridge.received_requests().await.ok_or("recorded")?;
    let request = asked.first().ok_or("one request")?;
    assert_eq!(None, request.url.query(), "nothing in the URL");
    assert!(String::from_utf8_lossy(&request.body).contains(MARKER));
    Ok(())
}

#[tokio::test]
async fn a_refusal_draws_its_status_and_diagnostics() -> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/$tofhir"))
        .respond_with(ResponseTemplate::new(422).set_body_raw(
            json!({"resourceType": "OperationOutcome", "issue": [{"severity": "error", "code": "processing", "diagnostics": "no program maps the synthetic template"}]}).to_string(),
            "application/fhir+json",
        ))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let html = to_fhir(&service, &cookie).await?;
    assert!(
        html.contains("422: the bridge refused the request."),
        "{html}"
    );
    assert!(
        html.contains("no program maps the synthetic template"),
        "{html}"
    );
    Ok(())
}

#[tokio::test]
async fn an_operations_lane_that_is_off_draws_its_404() -> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let html = to_fhir(&service, &cookie).await?;
    assert!(
        html.contains("404: the bridge refused the request."),
        "{html}"
    );
    Ok(())
}

#[tokio::test]
async fn toopenehr_draws_the_composition_and_its_issues_by_severity() -> Result<(), Box<dyn Error>>
{
    let bridge = MockServer::start().await;
    let answer = json!({"resourceType": "Parameters", "parameter": [
        {"name": "composition", "valueString": "{\"_type\":\"COMPOSITION\",\"archetype_node_id\":\"openEHR-EHR-COMPOSITION.synthetic.v1\"}"},
        {"name": "outcome", "resource": {"resourceType": "OperationOutcome", "issue": [
            {"severity": "information", "code": "informational", "diagnostics": "synthetic information"},
            {"severity": "warning", "code": "incomplete", "diagnostics": "synthetic warning"}
        ]}}
    ]});
    Mock::given(method("POST"))
        .and(path("/fhir/$toopenehr"))
        .and(body_string_contains("\"name\":\"bundle\""))
        .and(body_string_contains("\"valueCode\":\"flat\""))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(answer.to_string(), "application/fhir+json"),
        )
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let bundle = bundle();
    let html = ran(
        &service,
        "/api/toopenehr",
        &[
            ("bundle", bundle.as_str()),
            ("template_id", ""),
            ("format", "flat"),
        ],
        &cookie,
    )
    .await?;
    assert!(
        html.contains("openEHR-EHR-COMPOSITION.synthetic.v1"),
        "{html}"
    );
    let warning = html.find("synthetic warning").ok_or("the warning")?;
    let information = html
        .find("synthetic information")
        .ok_or("the information")?;
    assert!(
        warning < information,
        "a warning before information: {html}"
    );
    assert!(html.contains("The Parameters as JSON"), "{html}");
    Ok(())
}

#[tokio::test]
async fn an_issue_that_speaks_through_its_details_draws_its_text_and_place()
-> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    let answer = json!({"resourceType": "Parameters", "parameter": [
        {"name": "composition", "valueString": "{\"_type\":\"COMPOSITION\"}"},
        {"name": "outcome", "resource": {"resourceType": "OperationOutcome", "issue": [
            {"severity": "information", "code": "informational", "details": {
                "coding": [{"system": "https://example.org/synthetic-path", "code": "/content[openEHR-EHR-EVALUATION.synthetic.v1]"}],
                "text": "a synthetic element the mapping does not carry"
            }}
        ]}}
    ]});
    Mock::given(method("POST"))
        .and(path("/fhir/$toopenehr"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(answer.to_string(), "application/fhir+json"),
        )
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let bundle = bundle();
    let html = ran(
        &service,
        "/api/toopenehr",
        &[
            ("bundle", bundle.as_str()),
            ("template_id", ""),
            ("format", "canonical"),
        ],
        &cookie,
    )
    .await?;
    assert!(
        html.contains("<div>a synthetic element the mapping does not carry</div>"),
        "{html}"
    );
    assert!(
        html.contains("<div><code>/content[openEHR-EHR-EVALUATION.synthetic.v1]</code></div>"),
        "{html}"
    );
    Ok(())
}

#[tokio::test]
async fn input_the_console_does_not_send_is_named_and_the_bridge_never_asked()
-> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for (route, pairs, said) in [
        (
            "/api/tofhir",
            vec![("composition", "  "), ("template_id", ""), ("patient", "")],
            "The composition is empty",
        ),
        (
            "/api/toopenehr",
            vec![
                ("bundle", "not json"),
                ("template_id", ""),
                ("format", "canonical"),
            ],
            "The Bundle is not JSON.",
        ),
        (
            "/api/toopenehr",
            vec![
                ("bundle", "{\"resourceType\":\"Patient\"}"),
                ("template_id", ""),
                ("format", "canonical"),
            ],
            "resourceType is Bundle",
        ),
        (
            "/api/toopenehr",
            vec![("bundle", "{}"), ("template_id", ""), ("format", "xml")],
            "The format is not canonical or flat.",
        ),
        (
            "/api/validate",
            vec![
                ("resource_type", "Patient"),
                ("resource", "{\"resourceType\":\"Observation\"}"),
            ],
            "not the type chosen",
        ),
        (
            "/api/validate",
            vec![
                ("resource_type", "Pa/tient"),
                ("resource", "{\"resourceType\":\"Pa/tient\"}"),
            ],
            "not a FHIR resource type name",
        ),
    ] {
        let html = ran(&service, route, &pairs, &cookie).await?;
        assert!(html.contains(said), "{route} {said}: {html}");
    }
    let asked = bridge.received_requests().await.ok_or("recorded")?;
    assert!(
        asked.is_empty(),
        "the bridge was asked {} times",
        asked.len()
    );
    Ok(())
}

#[tokio::test]
async fn validate_offers_the_declared_types_and_draws_the_outcome() -> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/metadata"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            json!({"resourceType": "CapabilityStatement", "status": "active", "date": "2026-10-17", "kind": "instance", "fhirVersion": "4.0.1", "format": ["json"],
                "rest": [{"mode": "server", "resource": [
                    {"type": "Patient", "operation": [{"name": "validate", "definition": "http://hl7.org/fhir/OperationDefinition/Resource-validate"}]},
                    {"type": "Device"}
                ]}]}).to_string(),
            "application/fhir+json",
        ))
        .mount(&bridge)
        .await;
    Mock::given(method("POST"))
        .and(path("/fhir/Patient/$validate"))
        .and(body_string_contains(MARKER))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            json!({"resourceType": "OperationOutcome", "issue": [
                {"severity": "information", "code": "informational", "diagnostics": "synthetic template named"},
                {"severity": "error", "code": "invalid", "diagnostics": "synthetic validator message"}
            ]}).to_string(),
            "application/fhir+json",
        ))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/validate", &cookie)?).await?;
    assert!(
        page.contains(r#"<option value="Patient">Patient</option>"#),
        "{page}"
    );
    assert!(!page.contains(r#"<option value="Device">"#), "{page}");
    let resource = json!({"resourceType": "Patient", "name": [{"text": MARKER}]}).to_string();
    let html = ran(
        &service,
        "/api/validate",
        &[
            ("resource_type", "Patient"),
            ("resource", resource.as_str()),
        ],
        &cookie,
    )
    .await?;
    let error = html
        .find("synthetic validator message")
        .ok_or("the error")?;
    let information = html
        .find("synthetic template named")
        .ok_or("the information")?;
    assert!(error < information, "an error before information: {html}");
    assert!(
        html.contains(r#"<span class="fui-badge fui-tone-bad">error</span>"#),
        "{html}"
    );
    Ok(())
}

#[tokio::test]
async fn a_plain_post_is_refused_before_the_bridge_is_asked() -> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let composition = composition();
    let request = Request::post("/api/tofhir")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "text/html,application/xhtml+xml")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", &cookie)
        .body(Body::from(form(&[
            ("composition", composition.as_str()),
            ("template_id", ""),
            ("patient", ""),
        ])))?;
    let (response, body) = send(&service, request).await?;
    assert_ne!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("PlainPost") || body.contains("reload the page"),
        "{body}"
    );
    let asked = bridge.received_requests().await.ok_or("recorded")?;
    assert!(asked.is_empty());
    Ok(())
}

#[tokio::test]
async fn no_payload_or_patient_reaches_a_log_line() -> Result<(), Box<dyn Error>> {
    let captured = Captured::default();
    let _guard = captured.logging();
    let bridge = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/$toopenehr"))
        .respond_with(ResponseTemplate::new(422).set_body_raw(
            json!({"resourceType": "OperationOutcome", "issue": [{"severity": "error", "code": "processing", "diagnostics": "refused"}]}).to_string(),
            "application/fhir+json",
        ))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    to_fhir(&service, &cookie).await?;
    let bundle = bundle();
    ran(
        &service,
        "/api/toopenehr",
        &[
            ("bundle", bundle.as_str()),
            ("template_id", ""),
            ("format", "canonical"),
        ],
        &cookie,
    )
    .await?;
    let logged = captured.text();
    assert!(
        !logged.contains(MARKER),
        "a log line carries the payload: {logged}"
    );
    assert!(
        !logged.contains("Patient/synthetic-1"),
        "a log line carries the patient: {logged}"
    );
    Ok(())
}

#[tokio::test]
async fn the_playground_forms_are_post_forms_with_autocomplete_off_disabled_until_hydrated()
-> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, get_with("/playground", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    for action in ["/api/tofhir", "/api/toopenehr"] {
        assert!(
            page.contains(&format!(
                r#"<form method="post" action="{action}" autocomplete="off""#
            )),
            "{action}: {page}"
        );
    }
    assert!(
        page.contains(r#"<label for="tofhir-composition" class="fui-label">"#),
        "{page}"
    );
    assert!(
        page.contains("disabled"),
        "the submit waits for the bundle: {page}"
    );
    Ok(())
}
