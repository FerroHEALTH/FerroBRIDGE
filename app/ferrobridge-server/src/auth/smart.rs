// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART discovery: the `.well-known/smart-configuration` document and the
//! `rest.security` of the `CapabilityStatement`, both naming the configured
//! authorization server.
//!
//! The document is served at `[base]/.well-known/smart-configuration`, the
//! location SMART App Launch 2.2.0 fixes, and at the server root for a client
//! that looks there (no specification governs the root copy: our own
//! design). It "SHALL be JSON, regardless of `Accept` headers"
//! (<https://hl7.org/fhir/smart-app-launch/conformance.html>).

use std::sync::Arc;

use axum::Router;
use axum::response::IntoResponse;
use axum::routing::get;
use serde_json::Map;
use serde_json::Value;

use crate::auth::Authority;
use crate::auth::guard::SMART_CONFIGURATION;
use crate::facade::BASE_PATH;
use crate::facade::capability::Security;

/// The capabilities this resource server enforces whatever the deployment
/// adds: v1 and v2 scope syntax, and `patient/` and `user/` scopes
/// (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
const ENFORCED: [&str; 4] = [
    "permission-v1",
    "permission-v2",
    "permission-patient",
    "permission-user",
];

/// The grant types an authorization server supports when its metadata names
/// none (RFC 8414 §2, `grant_types_supported`).
const DEFAULT_GRANT_TYPES: [&str; 2] = ["authorization_code", "implicit"];

/// Returns the routes of the discovery document.
pub fn routes(authority: &Authority) -> Router {
    let document = Arc::new(configuration(authority));
    let answer = move || {
        let document = Arc::clone(&document);
        async move { axum::Json(document.as_ref().clone()).into_response() }
    };
    Router::new()
        .route(
            &format!("{BASE_PATH}{SMART_CONFIGURATION}"),
            get(answer.clone()),
        )
        .route(SMART_CONFIGURATION, get(answer))
}

/// Returns the discovery document of `authority`.
///
/// `token_endpoint`, `grant_types_supported`, `capabilities` and
/// `code_challenge_methods_supported` are required; the rest is copied from
/// the authorization server's metadata when it names them. The PKCE list
/// never carries `plain`: "the `plain` method SHALL NOT be included".
#[must_use]
pub fn configuration(authority: &Authority) -> Value {
    let metadata = &authority.metadata;
    let mut document = Map::new();
    let mut put = |name: &str, value: Option<Value>| {
        if let Some(value) = value {
            document.insert(String::from(name), value);
        }
    };
    let text = |value: &Option<String>| value.clone().map(Value::String);
    let list = |value: &Option<Vec<String>>| {
        value
            .as_ref()
            .map(|items| Value::from(items.iter().map(String::as_str).collect::<Vec<_>>()))
    };
    put("issuer", Some(Value::String(metadata.issuer.clone())));
    put("jwks_uri", text(&metadata.jwks_uri));
    put(
        "authorization_endpoint",
        text(&metadata.authorization_endpoint),
    );
    put(
        "token_endpoint",
        Some(Value::String(authority.token_endpoint.clone())),
    );
    put(
        "token_endpoint_auth_methods_supported",
        list(&metadata.token_endpoint_auth_methods_supported),
    );
    put(
        "grant_types_supported",
        Some(
            list(&metadata.grant_types_supported)
                .unwrap_or_else(|| Value::from(DEFAULT_GRANT_TYPES.to_vec())),
        ),
    );
    put(
        "registration_endpoint",
        text(&metadata.registration_endpoint),
    );
    put("scopes_supported", list(&metadata.scopes_supported));
    put(
        "response_types_supported",
        list(&metadata.response_types_supported),
    );
    put(
        "introspection_endpoint",
        text(&metadata.introspection_endpoint),
    );
    put("revocation_endpoint", text(&metadata.revocation_endpoint));
    let challenge: Vec<&str> = metadata
        .code_challenge_methods_supported
        .iter()
        .flatten()
        .map(String::as_str)
        .filter(|method| *method != "plain")
        .collect();
    put(
        "code_challenge_methods_supported",
        Some(Value::from(challenge)),
    );
    let mut capabilities: Vec<&str> = ENFORCED.to_vec();
    for added in &authority.settings.capabilities {
        if !capabilities.contains(&added.as_str()) {
            capabilities.push(added);
        }
    }
    put("capabilities", Some(Value::from(capabilities)));
    Value::Object(document)
}

/// Returns the `rest.security` endpoints of `authority`.
#[must_use]
pub fn security(authority: &Authority) -> Security {
    let metadata = &authority.metadata;
    Security {
        authorize: metadata.authorization_endpoint.clone(),
        token: authority.token_endpoint.clone(),
        register: metadata.registration_endpoint.clone(),
        introspect: metadata.introspection_endpoint.clone(),
        revoke: metadata.revocation_endpoint.clone(),
    }
}
