// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The facade under `[auth] mode = "introspection"`: an opaque token the
//! issuer's introspection endpoint reports active reaches the handler, the
//! answer is held, and an inactive token or an endpoint that fails is
//! refused (RFC 7662 §2.2, §2.3, §4).

use std::error::Error;

use ferrobridge_server::smart::SmartSettings;
use ferrohealth_smart::client::ClientCredentials;
use ferrohealth_smart::client::Endpoint;
use ferrohealth_smart::resource::introspection::IntrospectionAuth;
use ferrohealth_smart::resource::introspection::IntrospectionSettings;
use http::StatusCode;
use secrecy::SecretString;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::AUDIENCE;
use super::Issuer;
use super::authority;
use super::bearing;
use super::challenge;
use super::get;
use super::now;
use super::secured;
use crate::facade::harness;
use crate::facade::raw;

/// The read every case sends: an id the map does not know, so a request the
/// guard admits answers the handler's `404`.
const UNKNOWN: &str = "/fhir/Condition/never-written";

/// The opaque token every case presents.
const TOKEN: &str = "synthetic-opaque-token";

/// Returns an issuer whose metadata names its introspection endpoint.
async fn issuer() -> Issuer {
    let issuer = Issuer::bare().await;
    issuer
        .mount_metadata("/.well-known/oauth-authorization-server")
        .await;
    issuer
}

/// Mounts `response` as the introspection answer of `issuer`, expecting
/// `times` calls.
async fn answer(issuer: &Issuer, response: ResponseTemplate, times: u64) {
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/introspect"))
        .and(matchers::body_string_contains(format!("token={TOKEN}")))
        .respond_with(response)
        .expect(times)
        .mount(&issuer.server)
        .await;
}

/// Returns the settings of a resource server introspecting `issuer`'s
/// tokens under client authentication.
fn settings(issuer: &Issuer) -> SmartSettings {
    let mut settings = issuer.settings();
    settings.issuers = Vec::new();
    settings.introspection = Some(IntrospectionSettings::new(
        issuer.uri(),
        AUDIENCE,
        IntrospectionAuth::Client(Box::new(ClientCredentials::new(
            Endpoint::Issuer(issuer.uri()),
            String::from("ferrobridge-rs"),
            SecretString::from("synthetic-secret"),
        ))),
    ));
    settings
}

/// Returns an active response of `issuer` granting `scope`.
fn active(issuer: &Issuer, scope: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "active": true,
        "scope": scope,
        "client_id": "synthetic-client",
        "sub": "synthetic-client-sub",
        "iss": issuer.uri(),
        "aud": AUDIENCE,
        "exp": now() + 300,
    }))
}

/// Sends the read under the opaque token and returns the status and the
/// challenge.
async fn read(app: axum::Router) -> Result<(StatusCode, Option<String>), Box<dyn Error>> {
    let response = raw(app, bearing(get(UNKNOWN), TOKEN)).await?;
    Ok((response.status(), challenge(&response)))
}

#[tokio::test]
async fn an_active_opaque_token_reaches_the_handler_and_is_held() -> Result<(), Box<dyn Error>> {
    let issuer = issuer().await;
    answer(&issuer, active(&issuer, "system/Condition.r"), 1).await;
    let harness = harness().await;
    let authority = authority(settings(&issuer)).await;
    for _ in 0..2 {
        let (status, challenge) = read(secured(&harness, &authority)).await?;
        assert_eq!(StatusCode::NOT_FOUND, status, "the guard admits the token");
        assert_eq!(None, challenge);
    }
    Ok(())
}

#[tokio::test]
async fn an_active_token_without_the_scope_is_forbidden() -> Result<(), Box<dyn Error>> {
    let issuer = issuer().await;
    answer(&issuer, active(&issuer, "system/Patient.r"), 1).await;
    let harness = harness().await;
    let app = secured(&harness, &authority(settings(&issuer)).await);
    let (status, _) = read(app).await?;
    assert_eq!(StatusCode::FORBIDDEN, status);
    Ok(())
}

#[tokio::test]
async fn an_inactive_token_is_refused_as_invalid() -> Result<(), Box<dyn Error>> {
    let issuer = issuer().await;
    answer(
        &issuer,
        ResponseTemplate::new(200).set_body_json(serde_json::json!({ "active": false })),
        1,
    )
    .await;
    let harness = harness().await;
    let app = secured(&harness, &authority(settings(&issuer)).await);
    let (status, challenge) = read(app).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status);
    assert_eq!(
        Some("Bearer error=\"invalid_token\""),
        challenge.as_deref(),
        "RFC 6750 §3.1 invalid_token"
    );
    Ok(())
}

#[tokio::test]
async fn an_endpoint_that_fails_answers_service_unavailable() -> Result<(), Box<dyn Error>> {
    let issuer = issuer().await;
    answer(&issuer, ResponseTemplate::new(500), 1).await;
    let harness = harness().await;
    let app = secured(&harness, &authority(settings(&issuer)).await);
    let (status, _) = read(app).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status);
    Ok(())
}
