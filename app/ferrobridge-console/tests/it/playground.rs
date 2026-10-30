// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The playground against a stub bridge: each tab a plain `POST` form whose
//! answer the page draws and nothing keeps, the payload in the body and
//! never a URL, input the console does not send named under its pane with
//! the bridge never asked, the bridge's refusal with its status, the lane
//! that is off, and no payload or patient in any log line.

use std::error::Error;

use http::StatusCode;
use serde_json::json;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::stub::{answer, bridge, info, signed_in, statement};
use crate::support::{Captured, get_with, header, send};

/// A plain post of `form` from the playground tab at `page`, as its form
/// sends it to the tab's own address, carrying `cookie`.
fn post(
    page: &str,
    form: &str,
    cookie: &str,
) -> Result<http::Request<axum::body::Body>, Box<dyn Error>> {
    crate::stub::post(page, form, cookie)
}

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

/// The form-urlencoded body of `pairs`.
fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

/// What the playground's tab at `route` draws for `pairs`.
async fn ran(
    service: &axum::Router,
    route: &str,
    pairs: &[(&str, &str)],
    cookie: &str,
) -> Result<String, Box<dyn Error>> {
    let (response, body) = send(service, post(route, &form(pairs), cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{route}: {body}");
    assert_eq!(
        "no-store",
        header(&response, "cache-control"),
        "a run's answer is never stored"
    );
    Ok(body)
}

/// Asks `$tofhir` with [`composition`], a template and a patient.
async fn to_fhir(service: &axum::Router, cookie: &str) -> Result<String, Box<dyn Error>> {
    let composition = composition();
    ran(
        service,
        "/playground",
        &[
            ("input", composition.as_str()),
            ("template", "synthetic.vital_signs.v1"),
            ("patient", "Patient/synthetic-1"),
        ],
        cookie,
    )
    .await
}

#[tokio::test]
async fn tofhir_sends_the_payload_in_the_body_and_draws_the_bundle() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
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
    assert!(html.contains("obs-1"), "{html}");
    assert!(html.contains("Answered"), "{html}");
    let asked = bridge.received_requests().await.ok_or("recorded")?;
    let request = asked
        .iter()
        .find(|request| request.url.path() == "/fhir/$tofhir")
        .ok_or("one request")?;
    assert_eq!(None, request.url.query(), "nothing in the URL");
    assert!(String::from_utf8_lossy(&request.body).contains(MARKER));
    Ok(())
}

#[tokio::test]
async fn a_refusal_draws_its_status_and_diagnostics() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
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
        html.contains("The bridge refused the request (422)."),
        "{html}"
    );
    assert!(
        html.contains("no program maps the synthetic template"),
        "{html}"
    );
    Ok(())
}

#[tokio::test]
async fn a_lane_that_is_off_disables_run_and_says_why() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    let mut off = info();
    off["lanes"] = json!([{"name": "operations", "enabled": false}]);
    answer(&server, "/health/info", 200, &off).await;
    let (service, cookie) = signed_in(&server).await?;
    let (_response, body) = send(&service, get_with("/playground", &cookie)?).await?;
    assert!(
        body.contains("Conversions are off on this bridge."),
        "{body}"
    );
    assert!(body.contains("disabled"), "{body}");
    Ok(())
}

#[tokio::test]
async fn toopenehr_draws_the_composition_it_answered() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/fhir/$toopenehr"))
        .and(body_string_contains("\"valueCode\":\"flat\""))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            json!({"resourceType": "Parameters", "parameter": [{"name": "composition", "valueString": "{\"synthetic/answer\":\"flat\"}"}]}).to_string(),
            "application/fhir+json",
        ))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let bundle = bundle();
    let html = ran(
        &service,
        "/playground/toopenehr",
        &[
            ("input", bundle.as_str()),
            ("template", ""),
            ("format", "flat"),
        ],
        &cookie,
    )
    .await?;
    assert!(html.contains("synthetic/answer"), "{html}");
    Ok(())
}

#[tokio::test]
async fn input_the_console_does_not_send_is_named_and_the_bridge_never_asked()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for (route, pairs, said) in [
        (
            "/playground",
            vec![("input", "  "), ("template", ""), ("patient", "")],
            "The composition is empty",
        ),
        (
            "/playground/toopenehr",
            vec![("input", "not json"), ("format", "canonical")],
            "The Bundle is not JSON.",
        ),
        (
            "/playground/toopenehr",
            vec![
                ("input", "{\"resourceType\":\"Patient\"}"),
                ("format", "canonical"),
            ],
            "resourceType is Bundle",
        ),
        (
            "/playground/toopenehr",
            vec![("input", "{}"), ("format", "xml")],
            "The format is not canonical or FLAT.",
        ),
        (
            "/playground/validate",
            vec![
                ("resource_type", "Observation"),
                ("input", "{\"resourceType\":\"Patient\"}"),
            ],
            "not the type chosen",
        ),
    ] {
        let html = ran(&service, route, &pairs, &cookie).await?;
        assert!(html.contains(said), "{route} {said}: {html}");
    }
    let asked = bridge.received_requests().await.ok_or("recorded")?;
    assert!(
        !asked
            .iter()
            .any(|request| request.method.as_str() == "POST"),
        "the bridge was asked to run"
    );
    Ok(())
}

#[tokio::test]
async fn validate_offers_the_declared_types_and_draws_the_outcome() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/fhir/Observation/$validate"))
        .and(body_string_contains(MARKER))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            json!({"resourceType": "OperationOutcome", "issue": [{"severity": "information", "code": "informational", "diagnostics": "synthetic all clear"}]}).to_string(),
            "application/fhir+json",
        ))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/playground/validate", &cookie)?).await?;
    assert!(page.contains(r#"<option value="Observation""#), "{page}");
    assert!(statement().to_string().contains("validate"));
    let resource =
        json!({"resourceType": "Observation", "status": "final", "code": {"text": MARKER}})
            .to_string();
    let html = ran(
        &service,
        "/playground/validate",
        &[
            ("resource_type", "Observation"),
            ("input", resource.as_str()),
        ],
        &cookie,
    )
    .await?;
    assert!(html.contains("synthetic all clear"), "{html}");
    Ok(())
}

#[tokio::test]
async fn no_payload_or_patient_reaches_a_log_line() -> Result<(), Box<dyn Error>> {
    let captured = Captured::default();
    let _guard = captured.logging();
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/fhir/$tofhir"))
        .respond_with(ResponseTemplate::new(422).set_body_raw(
            json!({"resourceType": "OperationOutcome", "issue": [{"severity": "error", "code": "processing", "diagnostics": "refused"}]}).to_string(),
            "application/fhir+json",
        ))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    to_fhir(&service, &cookie).await?;
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
async fn every_tab_is_a_post_form_carrying_its_own_address() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for route in [
        "/playground",
        "/playground/toopenehr",
        "/playground/validate",
    ] {
        let (_response, body) = send(&service, get_with(route, &cookie)?).await?;
        assert!(
            body.contains(&format!(
                r#"<form method="post" action="{route}" class="fui-panel""#
            )),
            "a post draws its answer on {route}: {body}"
        );
        assert!(body.contains(r#"name="input""#), "{route}");
    }
    Ok(())
}

#[tokio::test]
async fn toopenehr_draws_the_composition_decoded_and_the_issues_of_its_outcome()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/fhir/$toopenehr"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            json!({"resourceType": "Parameters", "parameter": [
                {"name": "composition", "valueString": "{\"_type\":\"COMPOSITION\",\"synthetic_member\":1}"},
                {"name": "outcome", "resource": {"resourceType": "OperationOutcome", "issue": [{"severity": "warning", "code": "incomplete", "diagnostics": "synthetic element left unmapped"}]}}
            ]}).to_string(),
            "application/fhir+json",
        ))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let bundle = bundle();
    let html = ran(
        &service,
        "/playground/toopenehr",
        &[("input", bundle.as_str()), ("format", "canonical")],
        &cookie,
    )
    .await?;
    assert!(html.contains("synthetic_member"), "{html}");
    assert!(
        !html.contains("valueString"),
        "the envelope is drawn: {html}"
    );
    assert!(html.contains("synthetic element left unmapped"), "{html}");
    Ok(())
}

#[tokio::test]
async fn the_template_is_a_choice_of_the_running_sets_templates() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for (route, unset) in [
        ("/playground", "From the composition"),
        ("/playground/toopenehr", "Chosen by the mapping"),
    ] {
        let (_response, page) = send(&service, get_with(route, &cookie)?).await?;
        assert!(
            page.contains(r#"<select id="pg-template" name="template""#),
            "{route}: {page}"
        );
        assert!(
            page.contains(r#"<option value="synthetic.vital_signs.v1""#),
            "{route}"
        );
        assert!(page.contains(unset), "{route}");
    }
    Ok(())
}
