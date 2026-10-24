// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The token check: a valid token passes, and every token that does not hold
//! answers the same `401` (RFC 6750 §3.1) with an `OperationOutcome` that
//! names no claim.

use std::error::Error;
use std::time::Duration;

use ferrohealth_smart::resource::TokenProfile;
use http::StatusCode;
use jsonwebtoken::Algorithm;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;

use super::Issuer;
use super::authority;
use super::bearing;
use super::challenge;
use super::ec_key;
use super::get;
use super::mint;
use super::mint_with;
use super::now;
use super::secured;
use crate::facade::harness;
use crate::facade::raw;

/// The read every case sends: an id the map does not know, so a request the
/// guard admits answers the handler's `404`.
const UNKNOWN: &str = "/fhir/Condition/never-written";

/// The diagnostics every refused token gets.
const REFUSED: &str = "the request carries no valid access token for this server";

/// Sends the read under `token` and returns the status, the challenge and
/// the body.
async fn read_with(
    app: axum::Router,
    token: Option<&str>,
) -> Result<(StatusCode, Option<String>, serde_json::Value), Box<dyn Error>> {
    let request = match token {
        Some(token) => bearing(get(UNKNOWN), token),
        None => get(UNKNOWN),
    };
    let response = raw(app, request).await?;
    let status = response.status();
    let challenge = challenge(&response);
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let body = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    Ok((status, challenge, body))
}

/// Asserts the generic refusal of an invalid token.
fn assert_refused((status, challenge, body): &(StatusCode, Option<String>, serde_json::Value)) {
    assert_eq!(StatusCode::UNAUTHORIZED, *status, "{body}");
    assert_eq!(
        Some("Bearer error=\"invalid_token\""),
        challenge.as_deref(),
        "RFC 6750 §3.1 invalid_token"
    );
    assert_eq!(Some("login"), body["issue"][0]["code"].as_str(), "{body}");
    assert_eq!(
        Some(REFUSED),
        body["issue"][0]["diagnostics"].as_str(),
        "{body}"
    );
}

#[tokio::test]
async fn a_valid_token_reaches_the_handler() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let app = secured(&harness, &authority(issuer.settings()).await);
    let token = mint(&key, &issuer.claims("system/Condition.r"));
    let (status, challenge, body) = read_with(app, Some(&token)).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    assert_eq!(None, challenge);
    Ok(())
}

#[tokio::test]
async fn a_missing_token_is_challenged_without_an_error_code() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let app = secured(&harness, &authority(issuer.settings()).await);
    let (status, challenge, body) = read_with(app, None).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "{body}");
    // NOTE: RFC 6750 §3.1, a request lacking authentication information gets
    // no error code in the challenge.
    assert_eq!(Some("Bearer"), challenge.as_deref());
    assert_eq!(Some("login"), body["issue"][0]["code"].as_str());
    Ok(())
}

#[tokio::test]
async fn another_scheme_is_a_missing_bearer_token() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let app = secured(&harness, &authority(issuer.settings()).await);
    let mut request = get(UNKNOWN);
    request.headers_mut().insert(
        http::header::AUTHORIZATION,
        "Basic c3ludGhldGljOnNlY3JldA==".parse()?,
    );
    let response = raw(app, request).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    assert_eq!(Some("Bearer"), challenge(&response).as_deref());
    Ok(())
}

#[tokio::test]
async fn every_invalid_token_gets_the_same_body_naming_no_claim() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let authority = authority(issuer.settings()).await;
    let now = now();
    let mut expired = issuer.claims("system/Condition.r");
    expired["iat"] = serde_json::json!(now - 7200);
    expired["exp"] = serde_json::json!(now - 3600);
    let mut early = issuer.claims("system/Condition.r");
    early["nbf"] = serde_json::json!(now + 3600);
    let mut foreign_issuer = issuer.claims("system/Condition.r");
    foreign_issuer["iss"] = serde_json::json!("https://another-issuer.invalid");
    let mut foreign_audience = issuer.claims("system/Condition.r");
    foreign_audience["aud"] = serde_json::json!("https://another-server.invalid/fhir");
    let mut future = issuer.claims("system/Condition.r");
    future["iat"] = serde_json::json!(now + 3600);
    let mut no_client = issuer.claims("system/Condition.r");
    no_client
        .as_object_mut()
        .ok_or("claims are an object")?
        .remove("client_id");
    let mut no_subject = issuer.claims("system/Condition.r");
    no_subject
        .as_object_mut()
        .ok_or("claims are an object")?
        .remove("sub");
    let cases = [
        ("expired", expired),
        ("not yet valid", early),
        ("another issuer", foreign_issuer),
        ("another audience", foreign_audience),
        ("issued in the future", future),
        ("no client_id under RFC 9068", no_client),
        ("no sub", no_subject),
    ];
    let mut bodies = Vec::new();
    for (case, claims) in cases {
        let answer = read_with(secured(&harness, &authority), Some(&mint(&key, &claims))).await?;
        assert_refused(&answer);
        let text = answer.2.to_string();
        for claim in ["exp", "nbf", "iss", "aud", "iat", "sub", "client_id"] {
            assert!(!text.contains(&format!("\"{claim}\"")), "{case}: {text}");
        }
        bodies.push(answer.2);
    }
    assert!(
        bodies.windows(2).all(|pair| pair[0] == pair[1]),
        "one body for every failure"
    );
    Ok(())
}

#[tokio::test]
async fn a_token_signed_by_another_key_under_a_known_kid_is_refused()
-> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let forger = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let app = secured(&harness, &authority(issuer.settings()).await);
    let token = mint(&forger, &issuer.claims("system/Condition.r"));
    assert_refused(&read_with(app, Some(&token)).await?);
    Ok(())
}

#[tokio::test]
async fn an_algorithm_outside_the_configured_set_is_refused() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k384", Algorithm::ES384);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let mut settings = issuer.settings();
    settings.issuers[0].algorithms = vec![Algorithm::RS256, Algorithm::ES256];
    let app = secured(&harness, &authority(settings).await);
    let token = mint(&key, &issuer.claims("system/Condition.r"));
    assert_refused(&read_with(app, Some(&token)).await?);
    Ok(())
}

#[tokio::test]
async fn a_symmetric_token_is_refused() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let mut settings = issuer.settings();
    settings.issuers[0].algorithms = vec![Algorithm::ES256];
    let app = secured(&harness, &authority(settings).await);
    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some(String::from("k1"));
    header.typ = Some(String::from("at+jwt"));
    let token = jsonwebtoken::encode(
        &header,
        &issuer.claims("system/Condition.r"),
        &EncodingKey::from_secret(b"synthetic-shared-secret"),
    )?;
    assert_refused(&read_with(app, Some(&token)).await?);
    Ok(())
}

#[tokio::test]
async fn a_token_that_is_no_jwt_is_refused() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let app = secured(&harness, &authority(issuer.settings()).await);
    assert_refused(&read_with(app, Some("an-opaque-token")).await?);
    Ok(())
}

#[tokio::test]
async fn the_rfc_9068_profile_refuses_a_plain_jwt_and_the_jwt_profile_takes_it()
-> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(String::from("k1"));
    let mut claims = issuer.claims("system/Condition.r");
    claims
        .as_object_mut()
        .ok_or("claims are an object")?
        .remove("client_id");
    let token = mint_with(&key, &header, &claims);
    let strict = secured(&harness, &authority(issuer.settings()).await);
    assert_refused(&read_with(strict, Some(&token)).await?);
    let mut settings = issuer.settings();
    settings.issuers[0].profile = TokenProfile::Jwt;
    let lenient = secured(&harness, &authority(settings).await);
    let (status, _, body) = read_with(lenient, Some(&token)).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn an_unknown_kid_reads_the_key_set_once_more_and_passes() -> Result<(), Box<dyn Error>> {
    let old = ec_key("old", Algorithm::ES256);
    let rotated = ec_key("rotated", Algorithm::ES256);
    let issuer = Issuer::bare().await;
    issuer
        .mount_metadata("/.well-known/oauth-authorization-server")
        .await;
    issuer.mount_keys(&[&old], Some(1)).await;
    issuer.mount_keys(&[&old, &rotated], None).await;
    let harness = harness().await;
    let app = secured(&harness, &authority(issuer.settings()).await);
    let token = mint(&rotated, &issuer.claims("system/Condition.r"));
    let (status, _, body) = read_with(app, Some(&token)).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    assert_eq!(2, issuer.key_reads().await, "the boot read and one more");
    Ok(())
}

#[tokio::test]
async fn an_unknown_kid_within_the_interval_reads_nothing_and_is_refused()
-> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let stranger = ec_key("stranger", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let mut settings = issuer.settings();
    settings.issuers[0].jwks_refresh_interval = Duration::from_secs(3600);
    let authority = authority(settings).await;
    for _ in 0..3 {
        let token = mint(&stranger, &issuer.claims("system/Condition.r"));
        assert_refused(&read_with(secured(&harness, &authority), Some(&token)).await?);
    }
    assert_eq!(1, issuer.key_reads().await, "only the boot read");
    Ok(())
}

#[tokio::test]
async fn a_key_set_that_cannot_be_read_again_answers_503() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let stranger = ec_key("stranger", Algorithm::ES256);
    let issuer = Issuer::bare().await;
    issuer
        .mount_metadata("/.well-known/oauth-authorization-server")
        .await;
    issuer.mount_keys(&[&key], Some(1)).await;
    issuer.mount_failing_keys().await;
    let harness = harness().await;
    let app = secured(&harness, &authority(issuer.settings()).await);
    let token = mint(&stranger, &issuer.claims("system/Condition.r"));
    let (status, _, body) = read_with(app, Some(&token)).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{body}");
    assert_eq!(Some("transient"), body["issue"][0]["code"].as_str());
    Ok(())
}

#[tokio::test]
async fn a_key_the_issuer_withdrew_stops_verifying_once_the_set_is_past_its_max_age()
-> Result<(), Box<dyn Error>> {
    // NOTE: RFC 7517 §5 has a resource server read the issuer's current key
    // set, so a key withdrawn from it verifies nothing after the next read.
    let withdrawn = ec_key("withdrawn", Algorithm::ES256);
    let kept = ec_key("kept", Algorithm::ES256);
    let issuer = Issuer::bare().await;
    issuer
        .mount_metadata("/.well-known/oauth-authorization-server")
        .await;
    issuer.mount_keys(&[&withdrawn, &kept], Some(1)).await;
    issuer.mount_keys(&[&kept], None).await;
    let harness = harness().await;
    let mut settings = issuer.settings();
    settings.issuers[0].jwks_max_age = Duration::from_millis(300);
    settings.issuers[0].jwks_refresh_interval = Duration::from_secs(3600);
    let authority = authority(settings).await;
    let old = mint(&withdrawn, &issuer.claims("system/Condition.r"));
    let (status, _, body) = read_with(secured(&harness, &authority), Some(&old)).await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        status,
        "the key still verifies: {body}"
    );
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_refused(&read_with(secured(&harness, &authority), Some(&old)).await?);
    assert_eq!(
        2,
        issuer.key_reads().await,
        "the boot read and the max-age read"
    );
    let current = mint(&kept, &issuer.claims("system/Condition.r"));
    let (status, _, body) = read_with(secured(&harness, &authority), Some(&current)).await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        status,
        "the kept key verifies: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn with_auth_off_the_facade_is_open() -> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    let (status, challenge, body) = read_with(harness.app(), None).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    assert_eq!(None, challenge);
    let response = raw(harness.app(), get("/fhir/.well-known/smart-configuration")).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

// NOTE: RFC 9068 §2.2 requires `sub`, and RFC 7519 §4.1.2 has it identify a
// principal, which an empty or whitespace-only value does not.
#[tokio::test]
async fn a_token_with_a_blank_sub_is_the_generic_401() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    for blank in ["", "  "] {
        let app = secured(&harness, &authority(issuer.settings()).await);
        let mut claims = issuer.claims("system/Condition.r");
        claims["sub"] = serde_json::json!(blank);
        assert_refused(&read_with(app, Some(&mint(&key, &claims))).await?);
    }
    Ok(())
}

// NOTE: RFC 6750 §2.1 fixes the `b64token` syntax, and §3.1 answers a token
// that is "malformed" with `invalid_token`.
#[tokio::test]
async fn a_bearer_value_outside_b64token_is_the_generic_401() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    for value in ["a,b", "a=b", "=abc"] {
        let app = secured(&harness, &authority(issuer.settings()).await);
        assert_refused(&read_with(app, Some(value)).await?);
    }
    Ok(())
}
