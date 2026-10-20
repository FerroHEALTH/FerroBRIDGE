// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART discovery and the issuer's metadata: the two documents a client
//! reads before it holds a token, and the boot refusals of an authorization
//! server the resource server cannot trust.

use std::error::Error as StdError;

use ferrobridge_server::smart::Smart;
use ferrobridge_server::smart::SmartError;
use ferrohealth_smart::metadata::DiscoveryError;
use ferrohealth_smart::resource::ResourceError;
use http::StatusCode;
use http::header;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::authority;
use super::bearing;
use super::ec_key;
use super::get;
use super::mint;
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
        // NOTE: SMART App Launch 2.2.0 conformance.html, Metadata, `issuer` is
        // "Required if the server's capabilities include `sso-openid-connect`;
        // otherwise, omitted".
        assert!(body.get("issuer").is_none(), "{path}: {body}");
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
        for expected in ["permission-v1", "permission-patient", "permission-user"] {
            assert!(
                capabilities.contains(&serde_json::json!(expected)),
                "{expected}: {body}"
            );
        }
        // NOTE: SMART App Launch 2.2.0 conformance.html, `permission-v2` is
        // "support for SMARTv2 granular scope syntax", which grants nothing here.
        assert!(
            !capabilities.contains(&serde_json::json!("permission-v2")),
            "{body}"
        );
        assert!(
            !capabilities.contains(&serde_json::json!("sso-openid-connect")),
            "{body}"
        );
        assert!(body["grant_types_supported"].is_array(), "{body}");
    }
    Ok(())
}

#[tokio::test]
async fn the_smart_configuration_names_the_issuer_only_with_openid_connect_sign_in()
-> Result<(), Box<dyn StdError>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let mut settings = issuer.settings();
    settings.options.capabilities = vec![String::from("sso-openid-connect")];
    let signing_on = authority(settings).await;
    let (_, _, body) = read_open(
        secured(&harness, &signing_on),
        "/fhir/.well-known/smart-configuration",
    )
    .await?;
    // NOTE: SMART App Launch 2.2.0 conformance.html, Metadata, `issuer` and
    // `jwks_uri` are required with `sso-openid-connect`.
    let uri = issuer.uri();
    assert_eq!(Some(uri.as_str()), body["issuer"].as_str(), "{body}");
    assert!(body["jwks_uri"].is_string(), "{body}");
    let capabilities = body["capabilities"].as_array().ok_or("capabilities")?;
    assert!(
        capabilities.contains(&serde_json::json!("sso-openid-connect")),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn the_smart_configuration_lists_the_scopes_the_bridge_honours()
-> Result<(), Box<dyn StdError>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    let silent = authority(issuer.settings()).await;
    let (_, _, body) = read_open(
        secured(&harness, &silent),
        "/fhir/.well-known/smart-configuration",
    )
    .await?;
    assert!(body.get("scopes_supported").is_none(), "{body}");
    let mut settings = issuer.settings();
    settings.options.scopes_supported = vec![
        String::from("system/Condition.rs"),
        String::from("__ferrobridge.operator"),
    ];
    let listing = authority(settings).await;
    let (_, _, body) = read_open(
        secured(&harness, &listing),
        "/fhir/.well-known/smart-configuration",
    )
    .await?;
    // NOTE: SMART App Launch 2.2.0 conformance.html, Metadata: "The server
    // SHALL support all scopes listed here".
    assert_eq!(
        serde_json::json!(["system/Condition.rs", "__ferrobridge.operator"]),
        body["scopes_supported"],
        "{body}"
    );
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
    assert!(Smart::discover(issuer.settings()).await.is_ok());
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
    let error = Smart::discover(issuer.settings())
        .await
        .expect_err("RFC 8414 §3.3 refuses another issuer");
    assert!(
        matches!(&error, SmartError::Issuer { source, .. } if matches!(source.as_ref(), ResourceError::Discovery(DiscoveryError::IssuerMismatch { .. }))),
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
    let error = Smart::discover(issuer.settings())
        .await
        .expect_err("a key set is needed");
    assert!(
        matches!(&error, SmartError::Issuer { source, .. } if matches!(source.as_ref(), ResourceError::MissingMember { member: "jwks_uri" })),
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
    let error = Smart::discover(issuer.settings())
        .await
        .expect_err("the key set is read at boot");
    assert!(
        matches!(&error, SmartError::Issuer { source, .. } if matches!(source.as_ref(), ResourceError::KeySetStatus { status, .. } if *status == StatusCode::INTERNAL_SERVER_ERROR)),
        "{error:?}"
    );
    Ok(())
}

#[tokio::test]
async fn metadata_without_a_token_endpoint_refuses_the_start() -> Result<(), Box<dyn StdError>> {
    // NOTE: SMART App Launch 2.2.0 conformance.html, `token_endpoint` is
    // REQUIRED in the discovery document the bridge serves.
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::bare().await;
    let mut metadata = issuer.metadata();
    metadata
        .as_object_mut()
        .ok_or("metadata is an object")?
        .remove("token_endpoint");
    issuer
        .mount_document("/.well-known/oauth-authorization-server", metadata)
        .await;
    issuer.mount_keys(&[&key], None).await;
    let error = Smart::discover(issuer.settings())
        .await
        .expect_err("the discovery document needs a token endpoint");
    assert!(
        matches!(&error, SmartError::NoTokenEndpoint { issuer: named } if *named == issuer.uri()),
        "{error:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_second_trusted_issuer_is_admitted_beside_the_first() -> Result<(), Box<dyn StdError>> {
    let first_key = ec_key("k1", Algorithm::ES256);
    let second_key = ec_key("k2", Algorithm::ES256);
    let first = Issuer::start(&[&first_key]).await;
    let second = Issuer::start(&[&second_key]).await;
    let harness = harness().await;
    let mut settings = first.settings();
    settings.issuers.push(second.issuer_settings());
    settings.options.capabilities = vec![String::from("sso-openid-connect")];
    let authority = authority(settings).await;
    for (issuer, key) in [(&first, &first_key), (&second, &second_key)] {
        let token = mint(key, &issuer.claims("system/Condition.r"));
        let request = bearing(get("/fhir/Condition/never-written"), &token);
        let response = raw(secured(&harness, &authority), request).await?;
        assert_eq!(StatusCode::NOT_FOUND, response.status(), "{}", issuer.uri());
    }
    let crossed = mint(&second_key, &first.claims("system/Condition.r"));
    let request = bearing(get("/fhir/Condition/never-written"), &crossed);
    let response = raw(secured(&harness, &authority), request).await?;
    assert_eq!(
        StatusCode::UNAUTHORIZED,
        response.status(),
        "a key of one issuer signs nothing for the other"
    );
    let path = "/fhir/.well-known/smart-configuration";
    let (_, _, body) = read_open(secured(&harness, &authority), path).await?;
    assert_eq!(
        Some(first.uri().as_str()),
        body["issuer"].as_str(),
        "the first issuer is the one the document names"
    );
    assert_eq!(
        Some(format!("{}/token", first.uri()).as_str()),
        body["token_endpoint"].as_str(),
        "the first issuer's token endpoint"
    );
    Ok(())
}
