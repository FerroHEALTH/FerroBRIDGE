// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every server function the console registers: each is a public endpoint,
//! so each is a `POST` alone, refuses a caller without a live session or
//! from another origin before it asks the bridge anything, and asks the
//! bridge with the signed-in operator's own access token (the Leptos book,
//! `server/25_server_functions`).

use std::error::Error;

use axum::body::Body;
use ferrobridge_testkit::oidc::{Provider, TokenAnswer};
use http::{Method, Request, StatusCode};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{OPERATOR_TOKEN, ORIGIN, configuration, console, get_with, send, sign_in};

/// Every server function the console registers, with the form body it
/// takes.
const SERVER_FUNCTIONS: [(&str, &str); 11] = [
    ("/api/bridge-status", ""),
    ("/api/capability", ""),
    ("/api/dashboard", ""),
    ("/api/file", "path=fhirconnect%2Fa.yml"),
    ("/api/files", ""),
    ("/api/mappings", ""),
    ("/api/theme", "theme=dark"),
    ("/api/tofhir", "composition=x&template_id=&patient="),
    ("/api/toopenehr", "bundle=x&template_id=&format=canonical"),
    ("/api/validate", "resource_type=Patient&resource=x"),
    ("/api/validate-types", ""),
];

/// A synthetic R4 `CapabilityStatement` of the bridge.
const STATEMENT: &str = r#"{
  "resourceType": "CapabilityStatement",
  "status": "active",
  "date": "2026-10-17",
  "kind": "instance",
  "software": { "name": "FerroBRIDGE", "version": "0.0.8" },
  "fhirVersion": "4.0.1",
  "format": ["json"]
}"#;

/// A stub bridge that answers the operator's token alone.
async fn bridge() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/metadata"))
        .and(header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(STATEMENT, "application/fhir+json"))
        .mount(&server)
        .await;
    server
}

/// The console over `bridge` and a signed-in session's cookie.
async fn signed_in(bridge: &MockServer) -> Result<(axum::Router, String), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let base = format!("{}/", bridge.uri());
    let (_state, service) = console(&configuration(&provider, &base, "", ""))?;
    let cookie = sign_in(&service, &provider, TokenAnswer::bearer(OPERATOR_TOKEN)).await?;
    Ok((service, cookie))
}

/// A `POST` of the server function at `route` with `form`, from the
/// console's own page, carrying `cookie` when given.
fn call(route: &str, form: &str, cookie: Option<&str>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post(route)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin");
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    Ok(request.body(Body::from(form.to_owned()))?)
}

/// How many requests `bridge` received.
async fn asked(bridge: &MockServer) -> Result<usize, Box<dyn Error>> {
    Ok(bridge
        .received_requests()
        .await
        .ok_or("the stub records requests")?
        .len())
}

#[test]
fn every_registered_server_function_is_a_post_and_is_held_by_these_tests() {
    let mut registered: Vec<(&str, Method)> = leptos::server_fn::axum::server_fn_paths().collect();
    registered.sort_by(|left, right| left.0.cmp(right.0));
    let listed: Vec<(&str, Method)> = SERVER_FUNCTIONS
        .iter()
        .map(|(route, _)| (*route, Method::POST))
        .collect();
    assert_eq!(listed, registered);
}

#[tokio::test]
async fn every_server_function_refuses_a_caller_without_a_live_session()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, _cookie) = signed_in(&bridge).await?;
    for (route, form) in SERVER_FUNCTIONS {
        for cookie in [None, Some("ferrobridge_viewer_session=forged")] {
            let (response, body) = send(&service, call(route, form, cookie)?).await?;
            assert_ne!(StatusCode::OK, response.status(), "{route} {cookie:?}");
            assert!(body.contains("SignedOut"), "{route} {cookie:?}: {body}");
        }
    }
    assert_eq!(0, asked(&bridge).await?);
    Ok(())
}

#[tokio::test]
async fn every_server_function_answers_a_live_session_as_the_operator() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for (route, form) in SERVER_FUNCTIONS {
        let (response, body) = send(&service, call(route, form, Some(&cookie))?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{route}: {body}");
        assert_eq!(
            Some("no-store"),
            response
                .headers()
                .get("cache-control")
                .and_then(|value| value.to_str().ok()),
            "{route}"
        );
        assert!(
            response.headers().get("content-encoding").is_none(),
            "{route}"
        );
        assert!(!body.contains(OPERATOR_TOKEN), "{route}: {body}");
    }
    let (_response, body) = send(&service, call("/api/bridge-status", "", Some(&cookie))?).await?;
    assert!(
        body.contains("FerroBRIDGE") && body.contains("4.0.1"),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_bridge_refusal_is_an_answer_with_its_status() -> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/metadata"))
        .respond_with(ResponseTemplate::new(403).set_body_string(
            r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"forbidden","diagnostics":"synthetic refusal"}]}"#,
        ))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, body) = send(&service, call("/api/bridge-status", "", Some(&cookie))?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("403") && body.contains("synthetic refusal"),
        "{body}"
    );
    let (_page, page) = send(&service, get_with("/about", &cookie)?).await?;
    assert!(page.contains("Bridge refused 403"), "the top bar: {page}");
    Ok(())
}

#[tokio::test]
async fn every_server_function_refuses_a_request_from_another_origin() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for (route, form) in SERVER_FUNCTIONS {
        for from in [
            vec![("sec-fetch-site", "cross-site")],
            vec![("origin", "https://attacker.example.net")],
            Vec::new(),
        ] {
            let mut request = Request::post(route)
                .header("content-type", "application/x-www-form-urlencoded")
                .header("cookie", &cookie);
            for (name, value) in &from {
                request = request.header(*name, *value);
            }
            let (response, body) =
                send(&service, request.body(Body::from(form.to_owned()))?).await?;
            assert_eq!(
                StatusCode::FORBIDDEN,
                response.status(),
                "{route} {from:?}: {body}"
            );
        }
        let request = Request::post(route)
            .header("content-type", "application/x-www-form-urlencoded")
            .header("cookie", &cookie)
            .header("origin", ORIGIN)
            .body(Body::from(form.to_owned()))?;
        let (response, body) = send(&service, request).await?;
        assert_eq!(StatusCode::OK, response.status(), "{route}: {body}");
    }
    Ok(())
}

// A server function is a POST alone: a GET would pass the origin check, and
// the session cookie travels on a cross-site top-level GET.
#[tokio::test]
async fn no_server_function_answers_a_get() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for (route, form) in SERVER_FUNCTIONS {
        let (response, body) =
            send(&service, get_with(&format!("{route}?{form}"), &cookie)?).await?;
        assert!(
            response.status().is_client_error(),
            "{route}: {} {body}",
            response.status()
        );
    }
    assert_eq!(0, asked(&bridge).await?);
    Ok(())
}
