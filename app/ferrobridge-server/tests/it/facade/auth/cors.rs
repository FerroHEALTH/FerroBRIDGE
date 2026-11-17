// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Cross-origin resource sharing on the guarded FHIR base.
//!
//! SMART App Launch 2.2.0, App Launch, §Considerations for Cross-Origin
//! Resource Sharing (CORS) support: any origin reaches the public discovery
//! endpoints (`.well-known/smart-configuration` and `metadata`), and a
//! client's registered origins reach the FHIR REST API endpoints.

use std::error::Error;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrobridge_server::facade::cors::Cors;
use ferrobridge_server::facade::cors::Origin;
use ferrobridge_server::state::AppState;
use ferrohealth_health::registry::Registry;
use http::HeaderMap;
use http::Request;
use http::StatusCode;
use http::header;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::authority;
use super::ec_key;
use crate::facade::Harness;
use crate::facade::harness;
use crate::facade::raw;
use crate::facade::server_settings;

/// The origin the Inferno SMART App Launch kit calls from.
const LISTED: &str = "http://localhost:4567";

/// An origin no deployment of these cases lists.
const UNLISTED: &str = "https://elsewhere.invalid";

/// Returns the shipped router over a facade admitting `origins`, guarded by
/// a stub authorization server.
async fn guarded(origins: &[&str]) -> Result<(Router, Issuer), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let authority = authority(issuer.settings()).await;
    let Harness { facade, .. } = harness().await;
    let facade = Arc::try_unwrap(facade).map_err(|_shared| "the facade is held once")?;
    let origins = origins
        .iter()
        .map(|origin| Origin::parse(origin))
        .collect::<Result<Vec<_>, _>>()?;
    let facade = Arc::new(facade.with_cors(Cors::new(origins)));
    let state = AppState::with_health(Registry::default())
        .with_facade(facade)
        .with_auth(authority);
    Ok((
        ferrobridge_server::router(Arc::new(state), &server_settings()),
        issuer,
    ))
}

/// Returns a `method` request of `path` from `origin`.
fn from(method: &str, path: &str, origin: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::ORIGIN, origin)
        .body(Body::empty())
        .expect("a legal request")
}

/// Returns the CORS-preflight request of a `GET` of `path` carrying a bearer
/// token, from `origin` (Fetch Standard, §3.2.2 HTTP requests).
fn preflight(path: &str, origin: &str) -> Request<Body> {
    let mut request = from("OPTIONS", path, origin);
    let headers = request.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_REQUEST_METHOD,
        http::HeaderValue::from_static("GET"),
    );
    headers.insert(
        header::ACCESS_CONTROL_REQUEST_HEADERS,
        http::HeaderValue::from_static("authorization"),
    );
    request
}

/// Returns the value of `name` in `headers`, as text.
fn text(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// Returns whether `headers` carry a `Vary` naming `Origin`.
fn varies_on_origin(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::VARY)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|name| name.trim().eq_ignore_ascii_case("origin"))
}

#[tokio::test]
async fn the_discovery_endpoints_answer_any_origin() -> Result<(), Box<dyn Error>> {
    let (app, _issuer) = guarded(&[]).await?;
    for path in [
        "/fhir/.well-known/smart-configuration",
        "/.well-known/smart-configuration",
        "/fhir/metadata",
    ] {
        let response = raw(app.clone(), from("GET", path, LISTED)).await?;
        assert_eq!(StatusCode::OK, response.status(), "{path}");
        let headers = response.headers();
        assert_eq!(
            Some("*"),
            text(headers, header::ACCESS_CONTROL_ALLOW_ORIGIN).as_deref(),
            "{path}"
        );
        assert_eq!(
            None,
            text(headers, header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
            "{path}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_preflight_of_a_discovery_endpoint_is_answered_for_any_origin()
-> Result<(), Box<dyn Error>> {
    let (app, _issuer) = guarded(&[]).await?;
    for path in ["/fhir/metadata", "/fhir/.well-known/smart-configuration"] {
        let response = raw(app.clone(), preflight(path, UNLISTED)).await?;
        assert_eq!(StatusCode::NO_CONTENT, response.status(), "{path}");
        let headers = response.headers();
        assert_eq!(
            Some("*"),
            text(headers, header::ACCESS_CONTROL_ALLOW_ORIGIN).as_deref(),
            "{path}"
        );
        assert_eq!(
            Some("GET"),
            text(headers, header::ACCESS_CONTROL_ALLOW_METHODS).as_deref(),
            "{path}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_rest_route_answers_a_listed_origin_by_name_without_credentials()
-> Result<(), Box<dyn Error>> {
    let (app, _issuer) = guarded(&[LISTED]).await?;
    let response = raw(app, from("GET", "/fhir/Condition/c-1", LISTED)).await?;
    // The guard's refusal stays readable to the app, so it can tell a
    // missing token from a network failure.
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    let headers = response.headers();
    assert_eq!(
        Some(LISTED),
        text(headers, header::ACCESS_CONTROL_ALLOW_ORIGIN).as_deref()
    );
    assert_eq!(
        None,
        text(headers, header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
    );
    assert!(varies_on_origin(headers), "{headers:?}");
    let exposed = text(headers, header::ACCESS_CONTROL_EXPOSE_HEADERS).ok_or("exposed headers")?;
    assert!(exposed.contains("WWW-Authenticate"), "{exposed}");
    assert!(exposed.contains("Location"), "{exposed}");
    Ok(())
}

#[tokio::test]
async fn a_preflight_of_a_rest_route_admits_a_listed_origin_and_its_bearer_token()
-> Result<(), Box<dyn Error>> {
    let (app, _issuer) = guarded(&[LISTED]).await?;
    for path in ["/fhir/Condition/c-1", "/fhir/Condition", "/fhir"] {
        let response = raw(app.clone(), preflight(path, LISTED)).await?;
        assert_eq!(StatusCode::NO_CONTENT, response.status(), "{path}");
        let headers = response.headers();
        assert_eq!(
            Some(LISTED),
            text(headers, header::ACCESS_CONTROL_ALLOW_ORIGIN).as_deref(),
            "{path}"
        );
        let methods = text(headers, header::ACCESS_CONTROL_ALLOW_METHODS).ok_or("methods")?;
        assert!(
            methods.contains("GET") && methods.contains("PUT"),
            "{methods}"
        );
        let allowed = text(headers, header::ACCESS_CONTROL_ALLOW_HEADERS).ok_or("headers")?;
        assert!(allowed.contains("Authorization"), "{allowed}");
        assert!(varies_on_origin(headers), "{path}");
        assert_eq!(
            None,
            text(headers, header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_rest_route_answers_an_unlisted_origin_with_no_cors() -> Result<(), Box<dyn Error>> {
    let (app, _issuer) = guarded(&[LISTED]).await?;
    let response = raw(app.clone(), from("GET", "/fhir/Condition/c-1", UNLISTED)).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    assert_eq!(
        None,
        text(response.headers(), header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
    assert!(varies_on_origin(response.headers()));
    let response = raw(app, preflight("/fhir/Condition/c-1", UNLISTED)).await?;
    assert_ne!(StatusCode::NO_CONTENT, response.status());
    assert_eq!(
        None,
        text(response.headers(), header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
    assert_eq!(
        None,
        text(response.headers(), header::ACCESS_CONTROL_ALLOW_METHODS)
    );
    Ok(())
}

#[tokio::test]
async fn a_deployment_listing_no_origin_answers_no_cors_on_a_rest_route()
-> Result<(), Box<dyn Error>> {
    let (app, _issuer) = guarded(&[]).await?;
    let response = raw(app.clone(), from("GET", "/fhir/Condition/c-1", LISTED)).await?;
    assert_eq!(
        None,
        text(response.headers(), header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
    let response = raw(app, preflight("/fhir/Condition/c-1", LISTED)).await?;
    assert_eq!(
        None,
        text(response.headers(), header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
    Ok(())
}

#[tokio::test]
async fn a_route_outside_the_fhir_base_carries_no_cors() -> Result<(), Box<dyn Error>> {
    let (app, _issuer) = guarded(&[LISTED]).await?;
    let response = raw(app, from("GET", "/health/liveness", LISTED)).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        None,
        text(response.headers(), header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
    Ok(())
}
