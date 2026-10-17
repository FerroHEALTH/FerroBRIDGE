// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The engine-state screens against a stub bridge: the dashboard over
//! `/health/info`, `/health/readiness` and `/operator/state`, the capability
//! screen over `/fhir/metadata` and the SMART discovery document, and the
//! mappings overview over `/operator/state`. A refused or unreachable call
//! draws its status or the fault in its section, never an empty view, and
//! every diagnostic reads into the mapping foundation's own type.

use std::error::Error;

use axum::body::Body;
use ferrobridge_testkit::oidc::{Provider, TokenAnswer};
use http::{Request, StatusCode};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{OPERATOR_TOKEN, configuration, console, get, get_with, send, sign_in};

/// The synthetic `GET /health/info` document.
fn info() -> serde_json::Value {
    json!({
        "product": "FerroBRIDGE",
        "version": "0.0.8",
        "build": {"commit": "0123456789abcdef", "commit_short": "0123456789ab", "built_at": "2026-10-17T00:00:00Z", "rustc": "rustc 1.98.1"},
        "pins": [{"name": "FHIRconnect", "version": "1.0.0"}, {"name": "OMOP CDM", "version": "5.4"}],
        "lanes": [
            {"name": "facade", "enabled": true},
            {"name": "hl7v2", "enabled": true, "listen": "0.0.0.0:2575", "up": false},
            {"name": "etl", "enabled": false}
        ]
    })
}

/// The synthetic `GET /operator/state` document, one context and one
/// diagnostic.
fn state() -> serde_json::Value {
    json!({
        "contexts": [{
            "context": "synthetic-observation",
            "file": "fhirconnect/contexts/observation.context.yml",
            "template": "synthetic.vital_signs.v1",
            "profile": "http://hl7.org/fhir/StructureDefinition/Observation",
            "profile_version": "4.0.1",
            "resource_type": "Observation",
            "models": [{
                "name": "synthetic-blood-pressure",
                "file": "fhirconnect/models/bp.model.yml",
                "extensions": [{"name": "synthetic-bp-extension", "file": null}]
            }],
            "search": "fhirconnect/contexts/observation.search.yml"
        }],
        "templates": [
            {"id": "synthetic.vital_signs.v1", "source": "directory", "file": "templates/vital_signs.opt"},
            {"id": "synthetic.lab.v1", "source": "cdr"}
        ],
        "omocl": {"files": ["omocl/measurement.yml"], "valid": true},
        "counts": {"fhirconnect_files": 3, "contexts": 2, "programs": 1, "templates": 2, "omocl_files": 1},
        "diagnostics": [{
            "file": "fhirconnect/contexts/broken.context.yml",
            "line": 4, "column": 3,
            "severity": "error", "code": "missing-key",
            "message": "the synthetic <script>alert(1)</script> key is missing",
            "mapping": null, "model_path": ["spec"]
        }]
    })
}

/// A synthetic R4 `CapabilityStatement` with one resource.
fn statement() -> serde_json::Value {
    json!({
        "resourceType": "CapabilityStatement",
        "status": "active",
        "date": "2026-10-17",
        "kind": "instance",
        "software": {"name": "FerroBRIDGE", "version": "0.0.8"},
        "fhirVersion": "4.0.1",
        "format": ["json"],
        "rest": [{
            "mode": "server",
            "security": {"cors": true, "service": [{"coding": [{"system": "http://terminology.hl7.org/CodeSystem/restful-security-service", "code": "SMART-on-FHIR"}]}]},
            "resource": [{
                "type": "Observation",
                "interaction": [{"code": "read"}, {"code": "search-type"}],
                "searchParam": [{"name": "code", "type": "token"}],
                "operation": [{"name": "tofhir", "definition": "http://example.org/OperationDefinition/tofhir"}]
            }],
            "interaction": [{"code": "transaction"}]
        }]
    })
}

/// Mounts `body` with `status` at `GET route` on `bridge`, for the
/// operator's token alone.
async fn answer(bridge: &MockServer, route: &str, status: u16, body: &serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(route))
        .and(header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(bridge)
        .await;
}

/// The console over `bridge` and a signed-in session's cookie.
async fn signed_in(bridge: &str) -> Result<(axum::Router, String), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (_state, service) = console(&configuration(&provider, bridge, "", ""))?;
    let cookie = sign_in(&service, &provider, TokenAnswer::bearer(OPERATOR_TOKEN)).await?;
    Ok((service, cookie))
}

/// The HTML the server function at `route` draws for the session `cookie`.
async fn drawn(
    service: &axum::Router,
    route: &str,
    cookie: &str,
) -> Result<String, Box<dyn Error>> {
    let request = Request::post(route)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", cookie)
        .body(Body::empty())?;
    let (response, body) = send(service, request).await?;
    assert_eq!(StatusCode::OK, response.status(), "{route}: {body}");
    Ok(serde_json::from_str::<String>(&body)?)
}

#[tokio::test]
async fn the_dashboard_draws_build_facts_lanes_pins_readiness_and_counts()
-> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    answer(&bridge, "/health/info", 200, &info()).await;
    answer(
        &bridge,
        "/health/readiness",
        503,
        &json!({"state": "down", "indicators": {"cdr": {"state": "down", "detail": "synthetic refusal"}, "terminology": {"state": "up"}}}),
    )
    .await;
    answer(&bridge, "/operator/state", 200, &state()).await;
    let (service, cookie) = signed_in(&format!("{}/", bridge.uri())).await?;
    let html = drawn(&service, "/api/dashboard", &cookie).await?;
    for expected in [
        "0123456789ab",
        "rustc 1.98.1",
        "FHIRconnect",
        "5.4",
        "0.0.0.0:2575",
        "none of its own",
        "Not ready",
        "synthetic refusal",
        "1 of 2",
    ] {
        assert!(html.contains(expected), "{expected}: {html}");
    }
    assert!(
        html.contains(r#"<span class="badge tone-danger">down</span>"#),
        "{html}"
    );
    assert!(!html.contains(OPERATOR_TOKEN));
    Ok(())
}

#[tokio::test]
async fn a_refused_section_draws_its_status_and_the_others_their_content()
-> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    answer(&bridge, "/health/info", 200, &info()).await;
    answer(
        &bridge,
        "/health/readiness",
        200,
        &json!({"state": "up", "indicators": {}}),
    )
    .await;
    answer(
        &bridge,
        "/operator/state",
        403,
        &json!({"resourceType": "OperationOutcome", "issue": [{"severity": "error", "code": "forbidden", "diagnostics": "the operator scope is missing"}]}),
    )
    .await;
    let (service, cookie) = signed_in(&format!("{}/", bridge.uri())).await?;
    let html = drawn(&service, "/api/dashboard", &cookie).await?;
    assert!(
        html.contains("403: the bridge refused the request."),
        "{html}"
    );
    assert!(html.contains("the operator scope is missing"), "{html}");
    assert!(html.contains("Ready"), "{html}");
    assert!(
        html.contains("The bridge registers no readiness indicator."),
        "{html}"
    );
    let overview = drawn(&service, "/api/mappings", &cookie).await?;
    assert!(
        overview.contains("403: the bridge refused the request."),
        "{overview}"
    );
    Ok(())
}

#[tokio::test]
async fn a_bridge_out_of_reach_is_said_in_every_section() -> Result<(), Box<dyn Error>> {
    let (service, cookie) = signed_in("http://127.0.0.1:9/").await?;
    for route in ["/api/dashboard", "/api/capability", "/api/mappings"] {
        let html = drawn(&service, route, &cookie).await?;
        assert!(
            html.contains("the bridge could not be reached"),
            "{route}: {html}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_answer_of_another_shape_is_named_never_drawn_empty() -> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    answer(
        &bridge,
        "/health/info",
        200,
        &json!({"product": "FerroBRIDGE"}),
    )
    .await;
    answer(
        &bridge,
        "/health/readiness",
        200,
        &json!({"state": "down", "indicators": {}}),
    )
    .await;
    let mut foreign = state();
    foreign["diagnostics"] = json!([{"message": "no shape"}]);
    answer(&bridge, "/operator/state", 200, &foreign).await;
    let (service, cookie) = signed_in(&format!("{}/", bridge.uri())).await?;
    let html = drawn(&service, "/api/dashboard", &cookie).await?;
    let unreadable =
        "<p role=\"alert\">the bridge answered 200 with a body this console cannot read</p>";
    assert!(
        html.contains(&format!("<h2>Bridge</h2>{unreadable}")),
        "an info without its version: {html}"
    );
    assert!(
        html.contains("carries no state"),
        "a 200 that says down: {html}"
    );
    assert!(
        html.contains(&format!("<h2>Mappings</h2>{unreadable}")),
        "a diagnostic without its position: {html}"
    );
    Ok(())
}

#[tokio::test]
async fn the_capability_screen_draws_resources_security_raw_json_and_discovery()
-> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    answer(&bridge, "/fhir/metadata", 200, &statement()).await;
    answer(
        &bridge,
        "/fhir/.well-known/smart-configuration",
        200,
        &json!({"issuer": "https://idp.example.org/realm", "authorization_endpoint": "https://idp.example.org/auth", "token_endpoint": "https://idp.example.org/token", "capabilities": ["permission-v2"], "scopes_supported": ["ferrobridge.operator"]}),
    )
    .await;
    let (service, cookie) = signed_in(&format!("{}/", bridge.uri())).await?;
    let html = drawn(&service, "/api/capability", &cookie).await?;
    for expected in [
        "<h2>Observation</h2>",
        "read, search-type",
        "<code>code</code>",
        "token",
        "$tofhir",
        "SMART-on-FHIR",
        "transaction",
        "The CapabilityStatement as JSON",
        "https://idp.example.org/token",
        "permission-v2",
        "ferrobridge.operator",
    ] {
        assert!(html.contains(expected), "{expected}: {html}");
    }
    Ok(())
}

#[tokio::test]
async fn discovery_a_bridge_without_tokens_does_not_serve_is_its_refusal()
-> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    answer(&bridge, "/fhir/metadata", 200, &statement()).await;
    let (service, cookie) = signed_in(&format!("{}/", bridge.uri())).await?;
    let html = drawn(&service, "/api/capability", &cookie).await?;
    assert!(html.contains("<h2>SMART discovery</h2>"), "{html}");
    assert!(
        html.contains("404: the bridge refused the request."),
        "{html}"
    );
    assert!(html.contains("<h2>Observation</h2>"), "{html}");
    Ok(())
}

#[tokio::test]
async fn the_mappings_overview_draws_contexts_templates_omocl_and_escaped_diagnostics()
-> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    answer(&bridge, "/operator/state", 200, &state()).await;
    let (service, cookie) = signed_in(&format!("{}/", bridge.uri())).await?;
    let html = drawn(&service, "/api/mappings", &cookie).await?;
    for expected in [
        "synthetic-observation",
        "fhirconnect/contexts/observation.context.yml",
        "synthetic.vital_signs.v1",
        "http://hl7.org/fhir/StructureDefinition/Observation | 4.0.1",
        "synthetic-blood-pressure",
        "synthetic-bp-extension",
        "fhirconnect/contexts/observation.search.yml",
        "template directory",
        "the CDR",
        "omocl/measurement.yml",
        "valid",
        "missing-key",
        "fhirconnect/contexts/broken.context.yml:4:3",
    ] {
        assert!(html.contains(expected), "{expected}: {html}");
    }
    assert!(
        !html.contains("<script>alert(1)</script>"),
        "escaped: {html}"
    );
    assert!(html.contains("&lt;script&gt;"), "{html}");
    Ok(())
}

#[tokio::test]
async fn each_page_carries_its_drawn_screen_and_a_visitor_reads_the_sign_in()
-> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    answer(&bridge, "/operator/state", 200, &state()).await;
    answer(&bridge, "/fhir/metadata", 200, &statement()).await;
    let (service, cookie) = signed_in(&format!("{}/", bridge.uri())).await?;
    let (response, page) = send(&service, get_with("/mappings", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(page.contains("synthetic-observation"), "{page}");
    assert!(page.contains("<h1>Mappings</h1>"), "{page}");
    let (_response, page) = send(&service, get_with("/capability", &cookie)?).await?;
    assert!(page.contains("<h2>Observation</h2>"), "{page}");
    let (_state, visitor) = console("")?;
    for path in ["/", "/capability", "/mappings"] {
        let (_response, page) = send(&visitor, get(path)?).await?;
        assert!(page.contains(">Sign in</a>"), "{path}: {page}");
        assert!(!page.contains("<h1>Mappings</h1>"), "{path}");
    }
    Ok(())
}
