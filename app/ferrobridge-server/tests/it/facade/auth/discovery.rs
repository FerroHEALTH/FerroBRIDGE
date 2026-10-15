// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART discovery and the issuer's metadata: the two documents a client
//! reads before it holds a token, and the boot refusals of an authorization
//! server the resource server cannot trust.

use std::error::Error as StdError;

use ferrobridge_server::auth::AuthError;
use ferrobridge_server::auth::Authority;
use http::StatusCode;
use http::header;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::authority;
use super::ec_key;
use super::get;
use super::secured;
use crate::facade::harness;
use crate::facade::raw;

/// Reads `path` through `app` without a token, returning the status, the
/// media type and the body.
async fn read_open(
    app: axum::Router,
    path: &str,
) -> Result<(StatusCode, Option<String>, serde_json::Value), Box<dyn StdError>> {
    let response = raw(app, get(path)).await?;
    let status = response.status();
    let media = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    Ok((status, media, serde_json::from_slice(&bytes)?))
}

#[tokio::test]
async fn the_smart_configuration_names_the_authorization_server_without_a_token()
-> Result<(), Box<dyn StdError>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let authority = authority(issuer.settings()).await;
    for path in [
        "/fhir/.well-known/smart-configuration",
        "/.well-known/smart-configuration",
    ] {
        let (status, media, body) = read_open(secured(&harness, &authority), path).await?;
        assert_eq!(StatusCode::OK, status, "{path}: {body}");
        // NOTE: SMART App Launch 2.2.0 conformance.html, the document "SHALL
        // be JSON".
        assert_eq!(Some("application/json"), media.as_deref(), "{path}");
        let uri = issuer.uri();
        assert_eq!(Some(uri.as_str()), body["issuer"].as_str());
        assert_eq!(
            Some(format!("{uri}/token").as_str()),
            body["token_endpoint"].as_str()
        );
        assert_eq!(
            Some(format!("{uri}/authorize").as_str()),
            body["authorization_endpoint"].as_str()
        );
        let challenge = body["code_challenge_methods_supported"]
            .as_array()
            .ok_or("a PKCE list")?;
        assert!(challenge.contains(&serde_json::json!("S256")), "{body}");
        assert!(
            !challenge.contains(&serde_json::json!("plain")),
            "plain SHALL NOT be listed: {body}"
        );
        let capabilities = body["capabilities"].as_array().ok_or("capabilities")?;
        for expected in ["permission-v2", "permission-v1", "permission-patient"] {
            assert!(
                capabilities.contains(&serde_json::json!(expected)),
                "{expected}: {body}"
            );
        }
        assert!(body["grant_types_supported"].is_array(), "{body}");
    }
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_states_the_smart_service_without_a_token()
-> Result<(), Box<dyn StdError>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let app = secured(&harness, &authority(issuer.settings()).await);
    let (status, _, body) = read_open(app, "/fhir/metadata").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let security = &body["rest"][0]["security"];
    assert_eq!(
        Some("http://terminology.hl7.org/CodeSystem/restful-security-service"),
        security["service"][0]["coding"][0]["system"].as_str()
    );
    assert_eq!(
        Some("SMART-on-FHIR"),
        security["service"][0]["coding"][0]["code"].as_str()
    );
    let uris = &security["extension"][0];
    assert_eq!(
        Some("http://fhir-registry.smarthealthit.org/StructureDefinition/oauth-uris"),
        uris["url"].as_str()
    );
    let members: Vec<(&str, &str)> = uris["extension"]
        .as_array()
        .ok_or("the oauth-uris members")?
        .iter()
        .filter_map(|member| Some((member["url"].as_str()?, member["valueUri"].as_str()?)))
        .collect();
    let uri = issuer.uri();
    assert!(
        members.contains(&("token", format!("{uri}/token").as_str())),
        "{members:?}"
    );
    assert!(
        members.contains(&("authorize", format!("{uri}/authorize").as_str())),
        "{members:?}"
    );
    assert!(
        members.contains(&("introspect", format!("{uri}/introspect").as_str())),
        "{members:?}"
    );
    Ok(())
}

#[tokio::test]
async fn an_open_facade_states_no_security() -> Result<(), Box<dyn StdError>> {
    let harness = harness().await;
    let (status, _, body) = read_open(harness.app(), "/fhir/metadata").await?;
    assert_eq!(StatusCode::OK, status);
    assert!(body["rest"][0].get("security").is_none(), "{body}");
    Ok(())
}

#[tokio::test]
async fn the_openid_connect_location_answers_when_rfc_8414_has_none()
-> Result<(), Box<dyn StdError>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::bare().await;
    issuer
        .mount_metadata("/.well-known/openid-configuration")
        .await;
    issuer.mount_keys(&[&key], None).await;
    assert!(Authority::discover(issuer.settings()).await.is_ok());
    Ok(())
}

#[tokio::test]
async fn metadata_naming_another_issuer_refuses_the_start() -> Result<(), Box<dyn StdError>> {
    let issuer = Issuer::bare().await;
    let mut metadata = issuer.metadata();
    metadata["issuer"] = serde_json::json!("https://another-issuer.invalid");
    issuer
        .mount_document("/.well-known/oauth-authorization-server", metadata)
        .await;
    let error = Authority::discover(issuer.settings())
        .await
        .expect_err("RFC 8414 §3.3 refuses another issuer");
    assert!(
        matches!(error, AuthError::IssuerMismatch { .. }),
        "{error:?}"
    );
    Ok(())
}

#[tokio::test]
async fn metadata_without_a_key_set_refuses_the_start() -> Result<(), Box<dyn StdError>> {
    let issuer = Issuer::bare().await;
    let mut metadata = issuer.metadata();
    metadata
        .as_object_mut()
        .ok_or("metadata is an object")?
        .remove("jwks_uri");
    issuer
        .mount_document("/.well-known/oauth-authorization-server", metadata)
        .await;
    let error = Authority::discover(issuer.settings())
        .await
        .expect_err("a key set is needed");
    assert!(
        matches!(error, AuthError::MissingMember { member: "jwks_uri" }),
        "{error:?}"
    );
    Ok(())
}

#[tokio::test]
async fn an_unreachable_key_set_refuses_the_start() -> Result<(), Box<dyn StdError>> {
    let issuer = Issuer::bare().await;
    issuer
        .mount_metadata("/.well-known/oauth-authorization-server")
        .await;
    issuer.mount_failing_keys().await;
    let error = Authority::discover(issuer.settings())
        .await
        .expect_err("the key set is read at boot");
    assert!(
        matches!(error, AuthError::KeySetStatus { status, .. } if status == StatusCode::INTERNAL_SERVER_ERROR),
        "{error:?}"
    );
    Ok(())
}
