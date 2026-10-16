// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A CDR credential the token endpoint will not issue, as the FHIR caller
//! sees it: a `502` whose `OperationOutcome` names the upstream refusal and
//! never the bridge's client secret.

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::cdr::oauth::Endpoint;
use ferrobridge_server::cdr::oauth::OAuthConfig;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use ferrobridge_server::health::Registry;
use ferrobridge_server::state::AppState;
use http::StatusCode;
use secrecy::SecretString;
use std::error::Error as StdError;
use std::sync::Arc;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::FIXTURES;
use super::call;
use super::client;
use super::condition;
use super::first_issue;
use super::handle;
use super::post_condition;
use super::server_settings;
use super::settings;

/// The token endpoint path the stub serves.
const TOKEN_PATH: &str = "/oauth/token";

/// The client secret the bridge presents, which no answer may carry.
const SECRET: &str = "synthetic-secret-never-on-the-wire";

/// Returns the application whose facade reaches `cdr` with a credential
/// fetched from the token endpoint under the base URI `authorization`.
async fn app_over(
    cdr: &MockServer,
    authorization: &str,
) -> Result<axum::Router, Box<dyn StdError>> {
    Mock::given(matchers::method("GET"))
        .and(matchers::path(
            "/definition/template/adl1.4/ferrobridge.diagnose.v1",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/xml")
                .set_body_string(ferrobridge_testkit::fixtures::DIAGNOSE_OPT),
        )
        .mount(cdr)
        .await;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client(cdr)).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let grant = OAuthConfig::new(
        Endpoint::Token(format!("{authorization}{TOKEN_PATH}").parse()?),
        String::from("bridge"),
        SecretString::from(SECRET),
    );
    let oauth =
        CdrClient::new(&CdrConfig::new(format!("{}/", cdr.uri()).parse()?).with_oauth(grant))?;
    let store = Arc::new(MemoryStore::new());
    let facade = Arc::new(Facade::new(compiled, handle(&store), oauth, settings()));
    let state = AppState::with_health(Registry::default()).with_facade(facade);
    Ok(ferrobridge_server::router(
        Arc::new(state),
        &server_settings(),
    ))
}

#[tokio::test]
async fn a_refused_client_credentials_grant_answers_502_naming_the_oauth_error()
-> Result<(), Box<dyn StdError>> {
    // NOTE: RFC 6749 §5.2 makes `invalid_client` the token endpoint's refusal
    // of the client's own credentials, a fault upstream of the facade.
    let cdr = MockServer::start().await;
    let authorization = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": "invalid_client",
            "error_description": "unknown client",
        })))
        .mount(&authorization)
        .await;
    let app = app_over(&cdr, &authorization.uri()).await?;

    let (status, body) = call(app, post_condition(&condition())).await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status);
    let issue = first_issue(&body);
    assert_eq!(Some("exception"), issue["code"].as_str());
    let diagnostics = issue["diagnostics"].as_str().unwrap_or_default();
    assert!(diagnostics.contains("401"), "{diagnostics}");
    assert!(diagnostics.contains("invalid_client"), "{diagnostics}");
    assert!(
        !body.to_string().contains(SECRET),
        "the client secret reached the wire"
    );
    let reached = cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/ehr")
        .count();
    assert_eq!(0, reached, "no CDR call is made without a credential");
    Ok(())
}

#[tokio::test]
async fn an_unreachable_token_endpoint_answers_502() -> Result<(), Box<dyn StdError>> {
    let cdr = MockServer::start().await;
    // A port bound and released at once has no listener, so the connect fails.
    let closed = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?;
    let app = app_over(&cdr, &format!("http://{closed}")).await?;

    let (status, body) = call(app, post_condition(&condition())).await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status);
    let diagnostics = first_issue(&body)["diagnostics"]
        .as_str()
        .unwrap_or_default();
    assert!(diagnostics.contains(TOKEN_PATH), "{diagnostics}");
    Ok(())
}
