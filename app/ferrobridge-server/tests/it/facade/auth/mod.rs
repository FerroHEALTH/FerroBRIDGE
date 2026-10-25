// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The facade as a SMART App Launch 2.x resource server, over a `wiremock`
//! authorization server.
//!
//! Every signing key is generated when the case runs and lives only in
//! memory: no key, real or synthetic, is committed.

mod audit;
mod basic;
mod batch;
mod committer;
mod compartment;
mod discovery;
mod forwarding;
mod grants;
mod introspection;
mod routes;
mod scope;
mod search;
mod token;

use std::sync::Arc;
use std::time::Duration;

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::ECDSA_P256_SHA256_FIXED_SIGNING;
use aws_lc_rs::signature::ECDSA_P384_SHA384_FIXED_SIGNING;
use aws_lc_rs::signature::EcdsaKeyPair;
use axum::Router;
use axum::body::Body;
use ferrobridge_server::health::Registry;
use ferrobridge_server::smart::Smart;
use ferrobridge_server::smart::SmartSettings;
use ferrobridge_server::state::AppState;
use ferrohealth_smart::resource::Settings;
use http::Request;
use http::header;
use jsonwebtoken::Algorithm;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::jwk::Jwk;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::Harness;
use super::server_settings;

/// The audience every token of these cases is addressed to.
pub(super) const AUDIENCE: &str = "http://ferrobridge.invalid/fhir";

/// The path the stub serves the key set at.
const JWKS_PATH: &str = "/jwks";

/// The token endpoint the stub's metadata names.
const TOKEN_PATH: &str = "/token";

/// One signing key, generated for the case.
pub(crate) struct SigningKey {
    /// The `kid` the key is published and used under.
    kid: String,
    /// The algorithm it signs with.
    alg: Algorithm,
    /// The private half.
    encoding: EncodingKey,
    /// The public half as a JWK.
    jwk: serde_json::Value,
}

/// Returns a fresh ECDSA key for `alg` (ES256 or ES384) published as `kid`.
pub(crate) fn ec_key(kid: &str, alg: Algorithm) -> SigningKey {
    let curve = match alg {
        Algorithm::ES384 => &ECDSA_P384_SHA384_FIXED_SIGNING,
        _ => &ECDSA_P256_SHA256_FIXED_SIGNING,
    };
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(curve, &SystemRandom::new()).expect("a key pair");
    let encoding = EncodingKey::from_ec_der(pkcs8.as_ref());
    let mut jwk =
        serde_json::to_value(Jwk::from_encoding_key(&encoding, alg).expect("a public JWK"))
            .expect("the JWK serializes");
    jwk["kid"] = serde_json::json!(kid);
    jwk["use"] = serde_json::json!("sig");
    SigningKey {
        kid: kid.to_owned(),
        alg,
        encoding,
        jwk,
    }
}

/// The stub authorization server.
pub(crate) struct Issuer {
    /// The server, kept alive for the case.
    server: MockServer,
}

impl Issuer {
    /// Starts an issuer whose RFC 8414 metadata names the key set holding
    /// `keys`.
    pub(crate) async fn start(keys: &[&SigningKey]) -> Self {
        let issuer = Self::bare().await;
        issuer
            .mount_metadata("/.well-known/oauth-authorization-server")
            .await;
        issuer.mount_keys(keys, None).await;
        issuer
    }

    /// Starts an issuer that serves nothing yet.
    pub(super) async fn bare() -> Self {
        Self {
            server: MockServer::start().await,
        }
    }

    /// Returns the issuer identifier.
    pub(crate) fn uri(&self) -> String {
        self.server.uri()
    }

    /// Returns the metadata document this issuer answers with.
    pub(super) fn metadata(&self) -> serde_json::Value {
        let uri = self.uri();
        serde_json::json!({
            "issuer": uri,
            "jwks_uri": format!("{uri}{JWKS_PATH}"),
            "authorization_endpoint": format!("{uri}/authorize"),
            "token_endpoint": format!("{uri}{TOKEN_PATH}"),
            "token_endpoint_auth_methods_supported": ["private_key_jwt", "client_secret_basic"],
            "grant_types_supported": ["authorization_code", "client_credentials"],
            "code_challenge_methods_supported": ["S256", "plain"],
            "introspection_endpoint": format!("{uri}/introspect"),
            "scopes_supported": ["openid", "system/*.rs"]
        })
    }

    /// Mounts the metadata document at `path`.
    pub(super) async fn mount_metadata(&self, path: &str) {
        self.mount_document(path, self.metadata()).await;
    }

    /// Mounts `document` as the JSON answer of `path`.
    pub(super) async fn mount_document(&self, path: &str, document: serde_json::Value) {
        Mock::given(matchers::method("GET"))
            .and(matchers::path(path))
            .respond_with(ResponseTemplate::new(200).set_body_json(document))
            .mount(&self.server)
            .await;
    }

    /// Mounts the key set holding `keys`, answered `times` times when given.
    pub(super) async fn mount_keys(&self, keys: &[&SigningKey], times: Option<u64>) {
        let set: Vec<serde_json::Value> = keys.iter().map(|key| key.jwk.clone()).collect();
        let mock = Mock::given(matchers::method("GET"))
            .and(matchers::path(JWKS_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "keys": set
            })));
        match times {
            Some(times) => mock.up_to_n_times(times).mount(&self.server).await,
            None => mock.mount(&self.server).await,
        }
    }

    /// Mounts a key set location that answers `500`.
    pub(super) async fn mount_failing_keys(&self) {
        Mock::given(matchers::method("GET"))
            .and(matchers::path(JWKS_PATH))
            .respond_with(ResponseTemplate::new(500))
            .mount(&self.server)
            .await;
    }

    /// Returns how many times the key set was read.
    pub(super) async fn key_reads(&self) -> usize {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path() == JWKS_PATH)
            .count()
    }

    /// Returns the settings of a resource server for this issuer, reading
    /// the key set again on every unknown `kid`.
    pub(crate) fn settings(&self) -> SmartSettings {
        SmartSettings::new(self.issuer_settings())
    }

    /// Returns the resource-server settings of this issuer alone, reading the
    /// key set again on every unknown `kid`.
    pub(crate) fn issuer_settings(&self) -> Settings {
        let mut settings = Settings::new(self.uri(), AUDIENCE);
        settings.jwks_refresh_interval = Duration::ZERO;
        settings
    }

    /// Returns the claims of a valid token granting `scope`.
    pub(crate) fn claims(&self, scope: &str) -> serde_json::Value {
        let now = now();
        serde_json::json!({
            "iss": self.uri(),
            "aud": AUDIENCE,
            "sub": "synthetic-client-sub",
            "client_id": "synthetic-client",
            "iat": now,
            "exp": now + 300,
            "jti": "synthetic-jti-1",
            "scope": scope
        })
    }
}

/// Returns the current time in seconds.
pub(crate) fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// Returns `claims` signed by `key` with `typ` of `at+jwt`.
pub(crate) fn mint(key: &SigningKey, claims: &serde_json::Value) -> String {
    let mut header = Header::new(key.alg);
    header.kid = Some(key.kid.clone());
    header.typ = Some(String::from("at+jwt"));
    mint_with(key, &header, claims)
}

/// Returns `claims` signed by `key` under `header`.
pub(super) fn mint_with(key: &SigningKey, header: &Header, claims: &serde_json::Value) -> String {
    jsonwebtoken::encode(header, claims, &key.encoding).expect("the token signs")
}

/// Returns the resource server `settings` describe.
pub(crate) async fn authority(settings: SmartSettings) -> Arc<Smart> {
    Arc::new(
        Smart::discover(settings)
            .await
            .expect("the authorization server is discovered"),
    )
}

/// Returns the shipped router over `harness` with `authority` guarding it.
pub(super) fn secured(harness: &Harness, authority: &Arc<Smart>) -> Router {
    let state = AppState::with_health(Registry::default())
        .with_facade(Arc::clone(&harness.facade))
        .with_auth(Arc::clone(authority));
    ferrobridge_server::router(Arc::new(state), &server_settings())
}

/// Returns `request` carrying `token` as its bearer credential.
pub(crate) fn bearing(mut request: Request<Body>, token: &str) -> Request<Body> {
    request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}")
            .parse()
            .expect("a legal header value"),
    );
    request
}

/// Returns a `GET` of `path`.
pub(super) fn get(path: &str) -> Request<Body> {
    Request::get(path)
        .body(Body::empty())
        .expect("a legal request")
}

/// Returns the `WWW-Authenticate` challenge of `response`.
pub(crate) fn challenge(response: &http::Response<Body>) -> Option<String> {
    response
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// Returns a Condition whose subject is the literal `Patient/{patient}`.
pub(super) fn condition_of(patient: &str) -> serde_json::Value {
    let mut resource = super::condition();
    resource["subject"] = serde_json::json!({ "reference": format!("Patient/{patient}") });
    resource
}
