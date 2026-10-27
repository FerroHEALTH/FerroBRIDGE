// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Readiness over a stubbed CDR: what the upstream answers decides the status.

use crate::support;
use axum::body::Body;
use ferrobridge_server::config::Config;
use ferrobridge_server::state::AppState;
use ferrobridge_testkit::net::closed_http_url;
use http::{Request, StatusCode};
use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use tower::ServiceExt as _;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The ADL 1.4 template list the CDR probe reads (ITS-REST 1.1.0 Definition API).
const PROBE_PATH: &str = "/v1/definition/template/adl1.4";

/// Returns the serving state of a deployment whose CDR is `base`.
async fn router(base: &str) -> Result<Arc<AppState>, Box<dyn Error>> {
    let text = format!("[cdr]\nbase_url = \"{base}/v1\"\ntimeout_ms = 2000\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    Ok(serving(AppState::build(&settings).await?))
}

/// Returns `state` as a process that serves, past its boot.
fn serving(state: AppState) -> Arc<AppState> {
    state.lifecycle().booted();
    Arc::new(state)
}

/// Sends `GET /health/readiness` through the router over `state`, checks the
/// open route names no indicator's detail, and returns its status with its
/// state and every indicator by name, each with the detail the registry
/// holds behind the route.
async fn readiness(
    state: Arc<AppState>,
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    let app = ferrobridge_server::router(Arc::clone(&state), &support::settings());
    let response = app
        .oneshot(Request::get("/health/readiness").body(Body::empty())?)
        .await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let served: serde_json::Value = serde_json::from_slice(&bytes)?;
    let listed = served["indicators"].as_array().ok_or("an indicator list")?;
    assert!(
        listed
            .iter()
            .all(|indicator| indicator.get("detail").is_none()),
        "the open route names no detail: {served}"
    );
    let detailed = state.health().readiness(state.lifecycle()).await;
    let indicators: serde_json::Map<String, serde_json::Value> = detailed
        .indicators
        .iter()
        .map(|indicator| {
            (
                indicator.name.clone(),
                serde_json::json!({ "state": indicator.state, "detail": indicator.detail }),
            )
        })
        .collect();
    assert_eq!(listed.len(), indicators.len(), "{served}");
    Ok((
        status,
        serde_json::json!({ "state": served["state"], "indicators": indicators }),
    ))
}

#[tokio::test]
async fn a_cdr_that_answers_makes_readiness_two_hundred() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PROBE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let (status, document) = readiness(router(&server.uri()).await?).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(Some("up"), document["state"].as_str());
    assert_eq!(Some("up"), document["indicators"]["cdr"]["state"].as_str());
    Ok(())
}

#[tokio::test]
async fn a_cdr_that_answers_five_hundred_and_three_makes_readiness_five_hundred_and_three()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PROBE_PATH))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let (status, document) = readiness(router(&server.uri()).await?).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status);
    assert_eq!(Some("down"), document["state"].as_str());
    assert_eq!(
        Some("down"),
        document["indicators"]["cdr"]["state"].as_str()
    );
    assert!(
        document["indicators"]["cdr"]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("503")),
        "the detail names the upstream status: {document}"
    );
    Ok(())
}

#[tokio::test]
async fn a_cdr_that_refuses_the_credentials_makes_readiness_five_hundred_and_three()
-> Result<(), Box<dyn Error>> {
    for refused in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(PROBE_PATH))
            .respond_with(ResponseTemplate::new(refused.as_u16()))
            .mount(&server)
            .await;

        let (status, document) = readiness(router(&server.uri()).await?).await?;
        assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{refused}");
        assert_eq!(
            Some("down"),
            document["indicators"]["cdr"]["state"].as_str(),
            "{refused}"
        );
        assert!(
            document["indicators"]["cdr"]["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("credentials")),
            "the detail says the credentials were refused: {document}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_cdr_token_that_cannot_be_obtained_makes_readiness_five_hundred_and_three()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": "invalid_client",
            "error_description": "unknown client",
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(PROBE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(0)
        .mount(&server)
        .await;

    let base = server.uri();
    let text = format!(
        concat!(
            "[cdr]\nbase_url = \"{base}/v1\"\ntimeout_ms = 2000\n\n",
            "[cdr.credentials.oauth]\ntoken_endpoint = \"{base}/oauth/token\"\n",
            "client_id = \"ferrobridge\"\nclient_secret = \"synthetic-secret\"\n",
        ),
        base = base
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let state = serving(AppState::build(&settings).await?);

    let (status, document) = readiness(state).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status);
    assert_eq!(
        Some("down"),
        document["indicators"]["cdr"]["state"].as_str()
    );
    let detail = document["indicators"]["cdr"]["detail"]
        .as_str()
        .unwrap_or_default();
    assert!(
        detail.contains("401") && detail.contains("invalid_client"),
        "the detail names the token endpoint's status and OAuth error: {document}"
    );
    assert!(!detail.contains("synthetic-secret"));
    Ok(())
}

#[tokio::test]
async fn a_cdr_that_never_answers_makes_readiness_five_hundred_and_three()
-> Result<(), Box<dyn Error>> {
    let closed = closed_http_url()?;
    let (status, document) = readiness(router(&closed).await?).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status);
    assert_eq!(
        Some("down"),
        document["indicators"]["cdr"]["state"].as_str()
    );
    assert!(
        document["indicators"]["cdr"]["detail"].as_str().is_some(),
        "a down indicator states why: {document}"
    );
    Ok(())
}

#[tokio::test]
async fn liveness_stays_two_hundred_while_an_upstream_is_down() -> Result<(), Box<dyn Error>> {
    let closed = closed_http_url()?;
    let app = ferrobridge_server::router(router(&closed).await?, &support::settings());
    let response = app
        .oneshot(Request::get("/health/liveness").body(Body::empty())?)
        .await?;
    assert_eq!(
        StatusCode::OK,
        response.status(),
        "liveness reports this process, never its upstreams"
    );
    Ok(())
}

/// Returns the router of a deployment whose only upstream is the CDM
/// database `url`.
async fn cdm_router(url: &str) -> Result<Arc<AppState>, Box<dyn Error>> {
    let text = format!("[cdm]\nurl = {url:?}\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    Ok(serving(AppState::build(&settings).await?))
}

#[tokio::test]
async fn an_unreachable_cdm_database_makes_readiness_five_hundred_and_three()
-> Result<(), Box<dyn Error>> {
    // A port taken and released at once has no listener behind it.
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let url = format!("postgres://bridge@127.0.0.1:{port}/cdm?sslmode=disable");

    let (status, document) = readiness(cdm_router(&url).await?).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status);
    assert_eq!(
        Some("down"),
        document["indicators"]["cdm"]["state"].as_str()
    );
    assert!(
        document["indicators"]["cdm"]["detail"].as_str().is_some(),
        "the indicator says why: {document}"
    );
    Ok(())
}

#[tokio::test]
async fn a_reachable_cdm_database_makes_readiness_two_hundred() -> Result<(), Box<dyn Error>> {
    if !ferrobridge_testkit::containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = ferrobridge_testkit::containers::postgres().await?;

    let (status, document) = readiness(cdm_router(postgres.url()).await?).await?;
    assert_eq!(StatusCode::OK, status, "{document}");
    assert_eq!(Some("up"), document["indicators"]["cdm"]["state"].as_str());
    Ok(())
}

#[tokio::test]
async fn without_a_cdm_section_readiness_carries_no_cdm_indicator() -> Result<(), Box<dyn Error>> {
    let settings = Config::from_sources(None, &BTreeMap::new())?.resolve()?;
    let state = AppState::build(&settings).await?;
    assert!(
        !state.health().names().contains(&"cdm"),
        "{:?}",
        state.health().names()
    );
    Ok(())
}
