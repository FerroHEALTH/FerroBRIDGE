// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The facade as a SMART App Launch 2.x resource server, over a `wiremock`
//! authorization server.
//!
//! Every signing key is generated when the case runs and lives only in
//! memory: no key, real or synthetic, is committed.

mod access_record;
mod app_state;
mod assurance;
mod audit;
mod basic;
mod batch;
mod committer;
mod compartment;
mod cors;
mod described;
mod discovery;
mod ferrosmart;
mod forwarding;
mod grants;
mod introspection;
mod launch_task;
mod ml_dsa;
mod onward;
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
use ferrobridge_server::smart::Smart;
use ferrobridge_server::smart::SmartSettings;
use ferrobridge_server::state::AppState;
use ferrohealth_health::registry::Registry;
use ferrohealth_smart::jws::algorithm::Algorithm as Verified;
use ferrohealth_smart::jws::key::PrivateKey;
use ferrohealth_smart::kind::IssuerKind;
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

/// The client every token of these cases names, the issuer's first-party
/// client.
pub(crate) const CLIENT: &str = "synthetic-client";

/// The path the stub serves the key set at.
const JWKS_PATH: &str = "/jwks";

/// The token endpoint the stub's metadata names.
const TOKEN_PATH: &str = "/token";

/// One signing key, generated for the case.
pub(crate) struct SigningKey {
    /// The `kid` the key is published and used under.
    kid: String,
    /// The private half, with the algorithm it signs with.
    signer: Signer,
    /// The public half as a JWK.
    jwk: serde_json::Value,
}

/// The private half of a [`SigningKey`].
enum Signer {
    /// An RSA or ECDSA key `jsonwebtoken` signs with.
    Jose(Algorithm, EncodingKey),
    /// An ML-DSA key (RFC 9964), which `jsonwebtoken` has no algorithm for.
    MlDsa(ferrohealth_smart::jws::key::SigningKey),
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
        signer: Signer::Jose(alg, encoding),
        jwk,
    }
}

/// Returns a fresh ML-DSA key of `alg`'s parameter set (RFC 9964 §5),
/// published as `kid` in an `AKP` JWK (§3).
pub(crate) fn ml_dsa_key(kid: &str, alg: Verified) -> SigningKey {
    let mut seed = [0_u8; 32];
    aws_lc_rs::rand::fill(&mut seed).expect("a random seed");
    let private = PrivateKey::from_ml_dsa_seed(alg, &seed).expect("an ML-DSA key");
    let signing = ferrohealth_smart::jws::key::SigningKey::new(private, alg).expect("a signer");
    let mut jwk = signing.public_jwk().to_value();
    jwk["kid"] = serde_json::json!(kid);
    jwk["use"] = serde_json::json!("sig");
    SigningKey {
        kid: kid.to_owned(),
        signer: Signer::MlDsa(signing),
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

    /// Returns the stub server, for a case that mounts its own answers.
    pub(crate) const fn server(&self) -> &MockServer {
        &self.server
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

    /// Returns the resource-server settings of this issuer alone, a SMART
    /// authorization server, reading the key set again on every unknown
    /// `kid`.
    pub(crate) fn issuer_settings(&self) -> Settings {
        let mut settings = Settings::new(self.uri(), AUDIENCE, IssuerKind::Smart);
        settings.jwks_refresh_interval = Duration::ZERO;
        settings
    }

    /// Returns the claims of a machine's client credentials token granting
    /// `scope`, whose `sub` is its client (RFC 9068 §2.2), so the issuer's
    /// profile classifies it as a system caller.
    pub(crate) fn machine_claims(&self, scope: &str) -> serde_json::Value {
        let mut claims = self.claims(scope);
        claims["sub"] = serde_json::json!(CLIENT);
        claims
    }

    /// Returns the claims of a valid token granting `scope`.
    pub(crate) fn claims(&self, scope: &str) -> serde_json::Value {
        let now = now();
        serde_json::json!({
            "iss": self.uri(),
            "aud": AUDIENCE,
            "sub": "synthetic-client-sub",
            "client_id": CLIENT,
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
    match key.signer {
        Signer::Jose(alg, _) => {
            let mut header = Header::new(alg);
            header.kid = Some(key.kid.clone());
            header.typ = Some(String::from("at+jwt"));
            mint_with(key, &header, claims)
        }
        Signer::MlDsa(ref signing) => {
            let mut header = ferrohealth_smart::jws::compact::Header::new(signing.algorithm());
            header.kid = Some(key.kid.clone());
            header.typ = Some(String::from("at+jwt"));
            ferrohealth_smart::jws::compact::encode(&header, claims, signing)
                .expect("the token signs")
        }
    }
}

/// Returns `claims` signed by `key`, an RSA or ECDSA key, under `header`.
pub(super) fn mint_with(key: &SigningKey, header: &Header, claims: &serde_json::Value) -> String {
    let encoding = match key.signer {
        Signer::Jose(_, ref encoding) => Some(encoding),
        Signer::MlDsa(_) => None,
    }
    .expect("jsonwebtoken signs with an RSA or ECDSA key alone");
    jsonwebtoken::encode(header, claims, encoding).expect("the token signs")
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
