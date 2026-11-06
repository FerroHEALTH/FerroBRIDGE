// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The facade's caller carried onto the CDR under `[cdr.credentials.onward]`:
//! a person's token exchanged per request at a `wiremock` token endpoint that
//! checks what it is sent, a machine kept on the bridge's own token, and a
//! refused exchange never sent under the bridge's credential.
//!
//! The grants are RFC 8693 §2.1 token exchange (`docs/specs/ietf-oauth/rfc8693.txt`,
//! the request of §2.3 Figure 2 and the answer of Figure 3), the Keycloak
//! "Example token exchange request" with `audience`, and the RFC 7523 §2.1
//! JWT bearer grant with the Microsoft identity platform's "On-behalf-of
//! flow" parameters (`docs/specs/entra-docs/`, "Second case" and its
//! "Success response example" and "Error response example").

use std::error::Error;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use http::StatusCode;
use http::header;
use jsonwebtoken::Algorithm;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Request;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::AUDIENCE;
use super::Issuer;
use super::SigningKey;
use super::authority;
use super::bearing;
use super::challenge;
use super::condition_of;
use super::ec_key;
use super::get;
use super::mint;
use super::now;
use super::secured;
use crate::facade::FIXTURES;
use crate::facade::Harness;
use crate::facade::VERSION_ONE;
use crate::facade::batch::mount_create_once;
use crate::facade::client;
use crate::facade::handle;
use crate::facade::post_condition;
use crate::facade::raw;
use crate::facade::settings;
use crate::facade::stub::mount_ehr;
use crate::facade::stub::mount_read;

/// The bridge's client id at the authorization server.
const BRIDGE_CLIENT: &str = "ferrobridge";

/// The audience, and the resource indicator, the CDR's tokens name.
const CDR_AUDIENCE: &str = "https://cdr.example/openehr";

/// The token the client credentials grant issues the bridge for itself.
const OWN_TOKEN: &str = "synthetic-bridge-own-token";

/// The grant type of token exchange, form-encoded (RFC 8693 §2.1).
const TOKEN_EXCHANGE: &str = "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange";

/// The grant type of the JWT bearer grant, form-encoded (RFC 7523 §2.1).
const JWT_BEARER: &str = "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer";

/// The `client_assertion_type` of `private_key_jwt`, form-encoded (RFC 7523
/// §2.2).
const ASSERTION_TYPE: &str =
    "client_assertion_type=urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer";

/// Which onward grant a case runs.
#[derive(Clone, Copy)]
enum Grant {
    /// RFC 8693 token exchange naming the CDR by `resource`.
    Standard,
    /// The Keycloak profile: token exchange naming the CDR by `audience`.
    Keycloak,
    /// The Entra ID profile: the on-behalf-of JWT bearer grant.
    Entra,
}

impl Grant {
    /// Returns the `[cdr.credentials.onward]` table of this grant at `issuer`.
    fn table(self, issuer: &str) -> String {
        let (profile, target) = match self {
            Self::Standard => ("rfc9068", format!("indicators = [\"{CDR_AUDIENCE}\"]")),
            Self::Keycloak => ("keycloak", format!("indicators = [\"{CDR_AUDIENCE}\"]")),
            Self::Entra => ("entra", String::from("scope = \"api://cdr/.default\"")),
        };
        // NOTE: the Keycloak profile refuses its issuer until the deployment lists its
        // service accounts, `service-account-CLIENT_ID` (Keycloak admin-cli guide).
        let keys = match self {
            Self::Keycloak => "app_only = [{ subjects = [\"service-account-ferrobridge\"] }]\n",
            Self::Standard | Self::Entra => "",
        };
        format!(
            concat!(
                "[cdr.credentials.onward]\n",
                "issuer = \"{issuer}\"\n",
                "kind = \"persons\"\n",
                "profile = \"{profile}\"\n",
                "token_endpoint = \"{issuer}/token\"\n",
                "audiences = [\"{audience}\"]\n",
                "{target}\n",
                "[cdr.credentials.onward.keys]\n",
                "max_onward_lifetime = 3600\n",
                "{keys}",
            ),
            issuer = issuer,
            profile = profile,
            audience = CDR_AUDIENCE,
            target = target,
            keys = keys,
        )
    }
}

/// One case's world: the authorization server, the facade over its stub CDR,
/// the key the bridge signs its client assertions with, and the caller's key.
struct World {
    /// The authorization server, issuer of the caller's and the onward tokens.
    issuer: Issuer,
    /// The key both kinds of token are signed with.
    key: SigningKey,
    /// The facade and its stub CDR.
    harness: Harness,
    /// The bridge's client assertion key, kept on disk for the case.
    _assertion_key: tempfile::NamedTempFile,
}

impl World {
    /// Starts a world whose CDR lane exchanges each caller under `grant`.
    async fn start(grant: Grant) -> Result<Self, Box<dyn Error>> {
        let key = ec_key("k1", Algorithm::ES256);
        let issuer = Issuer::start(&[&key]).await;
        let cdr = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path(
                "/definition/template/adl1.4/ferrobridge.diagnoses.v1",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "application/xml")
                    .set_body_string(ferrobridge_testkit::fixtures::DIAGNOSES_OPT),
            )
            .mount(&cdr)
            .await;
        let mut assertion_key = tempfile::NamedTempFile::new()?;
        assertion_key.write_all(
            rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P384_SHA384)?
                .serialize_pem()
                .as_bytes(),
        )?;
        let text = format!(
            concat!(
                "[cdr]\nbase_url = \"{cdr}/\"\n\n",
                "[cdr.credentials.oauth]\n",
                "token_endpoint = \"{issuer}/token\"\n",
                "client_id = \"{client}\"\n",
                "client_auth = \"private_key_jwt\"\n",
                "private_key_file = \"{key}\"\n",
                "resource = \"{audience}\"\n\n",
                "{onward}"
            ),
            cdr = cdr.uri(),
            issuer = issuer.uri(),
            client = BRIDGE_CLIENT,
            audience = CDR_AUDIENCE,
            key = assertion_key.path().display(),
            onward = grant.table(&issuer.uri()),
        );
        let resolved = Config::from_sources(Some(&text), &std::collections::BTreeMap::new())?
            .resolve(Process::Job)?;
        let config = resolved.cdr.ok_or("the CDR lane is on")?;
        let set = programs::read_set(std::path::Path::new(FIXTURES))?;
        let templates = programs::fetch_templates(&set, &client(&cdr)).await?;
        let compiled = programs::compile_set(&set, &templates)?;
        let store = Arc::new(MemoryStore::default());
        let facade = Arc::new(Facade::new(
            compiled,
            handle(&store),
            CdrClient::new(&config)?,
            settings(),
        ));
        Ok(Self {
            issuer,
            key,
            harness: Harness { cdr, store, facade },
            _assertion_key: assertion_key,
        })
    }

    /// Returns the claims of a person's token at the facade, naming the
    /// bridge in its `aud` beside the facade, as the Keycloak and Entra
    /// profiles require of a subject token.
    fn person_claims(&self) -> serde_json::Value {
        let mut claims = self.issuer.claims("user/Condition.cru");
        claims["aud"] = serde_json::json!([AUDIENCE, BRIDGE_CLIENT]);
        claims["oid"] = serde_json::json!("synthetic-oid-1");
        claims["tid"] = serde_json::json!("synthetic-tenant");
        claims
    }

    /// Returns the claims of the token the authorization server issues for
    /// the CDR on behalf of the person of `person_claims`, under `grant`,
    /// living `lifetime` seconds.
    fn issued_claims(&self, grant: Grant, lifetime: i64) -> serde_json::Value {
        let now = now();
        let mut claims = serde_json::json!({
            "iss": self.issuer.uri(),
            "aud": CDR_AUDIENCE,
            "sub": "synthetic-client-sub",
            "iat": now,
            "exp": now + lifetime,
            "jti": "synthetic-onward-jti",
        });
        match grant {
            Grant::Standard => claims["client_id"] = serde_json::json!(BRIDGE_CLIENT),
            Grant::Keycloak => claims["azp"] = serde_json::json!(BRIDGE_CLIENT),
            Grant::Entra => {
                // NOTE: Microsoft, "Access token claims reference": `sub` is pairwise per
                // application, so the person is the `oid` and `tid` pair.
                claims["sub"] = serde_json::json!("synthetic-pairwise-sub-for-the-cdr");
                claims["azp"] = serde_json::json!(BRIDGE_CLIENT);
                claims["oid"] = serde_json::json!("synthetic-oid-1");
                claims["tid"] = serde_json::json!("synthetic-tenant");
                claims["ver"] = serde_json::json!("2.0");
            }
        }
        claims
    }

    /// Mounts the token endpoint answering the onward grant with `issued`,
    /// and the client credentials grant with [`OWN_TOKEN`].
    async fn mount_token_endpoint(&self, grant: Grant, issued: &str, lifetime: i64) {
        let answer = match grant {
            Grant::Standard | Grant::Keycloak => serde_json::json!({
                "access_token": issued,
                "issued_token_type": "urn:ietf:params:oauth:token-type:access_token",
                "token_type": "Bearer",
                "expires_in": lifetime,
            }),
            Grant::Entra => serde_json::json!({
                "token_type": "Bearer",
                "scope": "api://cdr/.default",
                "expires_in": lifetime,
                "ext_expires_in": 0,
                "access_token": issued,
            }),
        };
        let grant_type = match grant {
            Grant::Standard | Grant::Keycloak => TOKEN_EXCHANGE,
            Grant::Entra => JWT_BEARER,
        };
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/token"))
            .and(matchers::body_string_contains(grant_type))
            .respond_with(ResponseTemplate::new(200).set_body_json(answer))
            .mount(self.issuer.server())
            .await;
        self.mount_own_token().await;
    }

    /// Mounts the client credentials grant answering [`OWN_TOKEN`].
    async fn mount_own_token(&self) {
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/token"))
            .and(matchers::body_string_contains(
                "grant_type=client_credentials",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": OWN_TOKEN,
                "token_type": "Bearer",
                "expires_in": 300,
            })))
            .mount(self.issuer.server())
            .await;
    }

    /// Returns the form body of every onward grant request the token
    /// endpoint received.
    async fn onward_requests(&self) -> Vec<String> {
        self.token_requests()
            .await
            .into_iter()
            .filter(|body| !body.contains("grant_type=client_credentials"))
            .collect()
    }

    /// Returns the form body of every request the token endpoint received.
    async fn token_requests(&self) -> Vec<String> {
        received(self.issuer.server(), "/token")
            .await
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .collect()
    }

    /// Returns the `Authorization` value of every CDR call after the
    /// template fetch, with its method and path.
    async fn cdr_calls(&self) -> Vec<(String, Option<String>)> {
        self.harness
            .cdr
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| !request.url.path().starts_with("/definition/"))
            .map(|request| {
                (
                    format!("{} {}", request.method, request.url.path()),
                    request
                        .headers
                        .get(header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_owned),
                )
            })
            .collect()
    }

    /// Mounts the CDR answers of a create and a read of `resource`.
    async fn mount_cdr(&self, resource: &serde_json::Value) {
        mount_ehr(&self.harness.cdr).await;
        let composition = self.harness.composition_of(resource);
        mount_create_once(&self.harness.cdr, VERSION_ONE, &composition).await;
        mount_read(&self.harness.cdr, VERSION_ONE, &composition).await;
    }
}

/// Returns the requests `server` received at `path`.
async fn received(server: &MockServer, path: &str) -> Vec<Request> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|request| request.url.path() == path)
        .collect()
}

/// Returns `text` form-encoded as a parameter value.
fn encoded(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

/// Asserts that every CDR call in `calls` carries `expected` as its bearer
/// token, and that `expected` reached the CDR at all.
fn assert_every_call_under(calls: &[(String, Option<String>)], expected: &str) {
    assert!(!calls.is_empty(), "no call reached the CDR");
    for (call, credential) in calls {
        assert_eq!(
            Some(format!("Bearer {expected}")).as_deref(),
            credential.as_deref(),
            "{call} carries the wrong credential"
        );
    }
}

/// Creates and reads a Condition as `token`, asserting both succeed.
async fn create_and_read(world: &World, token: &str) -> Result<(), Box<dyn Error>> {
    let authority = authority(world.issuer.settings()).await;
    let resource = condition_of("p-1");
    let created = raw(
        secured(&world.harness, &authority),
        bearing(post_condition(&resource), token),
    )
    .await?;
    assert_eq!(StatusCode::CREATED, created.status(), "the create");
    let bytes = axum::body::to_bytes(created.into_body(), 1024 * 1024).await?;
    let id = serde_json::from_slice::<serde_json::Value>(&bytes)?["id"]
        .as_str()
        .ok_or("an id")?
        .to_owned();
    let read = raw(
        secured(&world.harness, &authority),
        bearing(get(&format!("/fhir/Condition/{id}")), token),
    )
    .await?;
    assert_eq!(StatusCode::OK, read.status(), "the read");
    Ok(())
}

/// Reads a Condition id `c-1` as `token` and returns the response.
async fn read_once(
    world: &World,
    token: &str,
) -> Result<http::Response<axum::body::Body>, Box<dyn Error>> {
    let authority = authority(world.issuer.settings()).await;
    raw(
        secured(&world.harness, &authority),
        bearing(get("/fhir/Condition/c-1"), token),
    )
    .await
}

#[tokio::test]
async fn a_person_writes_and_reads_under_the_rfc_8693_exchanged_token() -> Result<(), Box<dyn Error>>
{
    let world = World::start(Grant::Standard).await?;
    let issued = mint(&world.key, &world.issued_claims(Grant::Standard, 300));
    world
        .mount_token_endpoint(Grant::Standard, &issued, 300)
        .await;
    world.mount_cdr(&condition_of("p-1")).await;
    let token = mint(&world.key, &world.person_claims());

    create_and_read(&world, &token).await?;

    assert_every_call_under(&world.cdr_calls().await, &issued);
    let requests = world.onward_requests().await;
    assert_eq!(1, requests.len(), "one exchange serves both requests");
    let body = requests.first().ok_or("an exchange")?;
    // RFC 8693 §2.3 Figure 2: the caller's token as `subject_token`, an access
    // token, and the backend named by `resource`.
    assert!(body.contains(TOKEN_EXCHANGE), "{body}");
    assert!(body.contains(&format!("subject_token={token}")), "{body}");
    assert!(
        body.contains(&format!(
            "subject_token_type={}",
            encoded("urn:ietf:params:oauth:token-type:access_token")
        )),
        "{body}"
    );
    assert!(
        body.contains(&format!("resource={}", encoded(CDR_AUDIENCE))),
        "{body}"
    );
    assert!(body.contains(ASSERTION_TYPE), "{body}");
    assert!(body.contains("client_assertion="), "{body}");
    assert!(!body.contains("actor_token"), "{body}");
    assert!(!body.contains("audience="), "{body}");
    assert!(
        world
            .token_requests()
            .await
            .iter()
            .all(|request| !request.contains("grant_type=client_credentials")),
        "a person's request never fetches the bridge's own token"
    );
    Ok(())
}

#[tokio::test]
async fn the_keycloak_exchange_names_the_cdr_by_audience() -> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Keycloak).await?;
    let issued = mint(&world.key, &world.issued_claims(Grant::Keycloak, 300));
    world
        .mount_token_endpoint(Grant::Keycloak, &issued, 300)
        .await;
    world.mount_cdr(&condition_of("p-1")).await;
    let token = mint(&world.key, &world.person_claims());

    create_and_read(&world, &token).await?;

    assert_every_call_under(&world.cdr_calls().await, &issued);
    let requests = world.onward_requests().await;
    let body = requests.first().ok_or("an exchange")?;
    assert!(body.contains(TOKEN_EXCHANGE), "{body}");
    assert!(body.contains(&format!("subject_token={token}")), "{body}");
    assert!(
        body.contains(&format!("audience={}", encoded(CDR_AUDIENCE))),
        "{body}"
    );
    assert!(
        body.contains(&format!(
            "requested_token_type={}",
            encoded("urn:ietf:params:oauth:token-type:access_token")
        )),
        "{body}"
    );
    assert!(!body.contains("resource="), "{body}");
    assert!(!body.contains("actor_token"), "{body}");
    Ok(())
}

#[tokio::test]
async fn the_entra_on_behalf_of_grant_names_the_person_by_oid() -> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Entra).await?;
    let issued = mint(&world.key, &world.issued_claims(Grant::Entra, 3269));
    world
        .mount_token_endpoint(Grant::Entra, &issued, 3269)
        .await;
    world.mount_cdr(&condition_of("p-1")).await;
    let token = mint(&world.key, &world.person_claims());

    create_and_read(&world, &token).await?;

    assert_every_call_under(&world.cdr_calls().await, &issued);
    let requests = world.onward_requests().await;
    let body = requests.first().ok_or("an exchange")?;
    // Microsoft, "On-behalf-of flow", "Second case": `assertion`,
    // `client_assertion`, `scope` and `requested_token_use`, with no actor.
    assert!(body.contains(JWT_BEARER), "{body}");
    assert!(body.contains(&format!("assertion={token}")), "{body}");
    assert!(body.contains("requested_token_use=on_behalf_of"), "{body}");
    assert!(
        body.contains(&format!("scope={}", encoded("api://cdr/.default"))),
        "{body}"
    );
    assert!(
        body.contains(&format!("client_id={BRIDGE_CLIENT}")),
        "{body}"
    );
    assert!(body.contains(ASSERTION_TYPE), "{body}");
    for absent in ["actor_token", "subject_token", "resource=", "audience="] {
        assert!(!body.contains(absent), "{absent} in {body}");
    }
    Ok(())
}

#[tokio::test]
async fn an_entra_token_naming_another_person_is_refused() -> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Entra).await?;
    let mut claims = world.issued_claims(Grant::Entra, 300);
    claims["oid"] = serde_json::json!("synthetic-oid-2");
    let issued = mint(&world.key, &claims);
    world.mount_token_endpoint(Grant::Entra, &issued, 300).await;
    let token = mint(&world.key, &world.person_claims());

    let response = read_once(&world, &token).await?;

    assert_eq!(StatusCode::BAD_GATEWAY, response.status());
    assert!(
        world.cdr_calls().await.is_empty(),
        "no call reached the CDR"
    );
    Ok(())
}

#[tokio::test]
async fn an_entra_interaction_required_answer_is_a_401_with_the_claims_challenge()
-> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Entra).await?;
    world.mount_own_token().await;
    // Microsoft, "On-behalf-of flow", "Error response example".
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .and(matchers::body_string_contains(JWT_BEARER))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "interaction_required",
            "error_description": "AADSTS50079: synthetic multifactor requirement.",
            "error_codes": [50079],
            "claims": "{\"access_token\":{\"polids\":{\"essential\":true,\"values\":[\"00aa00aa-bb11-cc22-dd33-44ee44ee44ee\"]}}}"
        })))
        .mount(world.issuer.server())
        .await;
    let token = mint(&world.key, &world.person_claims());

    let response = read_once(&world, &token).await?;

    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    let challenge = challenge(&response).ok_or("a WWW-Authenticate challenge")?;
    assert!(
        challenge.starts_with("Bearer error=\"interaction_required\""),
        "{challenge}"
    );
    assert!(challenge.contains("claims=\""), "{challenge}");
    assert!(challenge.contains("polids"), "{challenge}");
    assert!(
        world.cdr_calls().await.is_empty(),
        "no call reached the CDR"
    );
    Ok(())
}

#[tokio::test]
async fn the_exchanged_token_is_reused_while_it_lives() -> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Standard).await?;
    let resource = condition_of("p-1");
    world.mount_cdr(&resource).await;
    let issued = mint(&world.key, &world.issued_claims(Grant::Standard, 300));
    world
        .mount_token_endpoint(Grant::Standard, &issued, 300)
        .await;
    let token = mint(&world.key, &world.person_claims());

    create_and_read(&world, &token).await?;
    let _again = read_once(&world, &token).await?;

    assert_eq!(1, world.onward_requests().await.len());
    assert_every_call_under(&world.cdr_calls().await, &issued);
    Ok(())
}

#[tokio::test]
async fn an_expiring_exchanged_token_is_exchanged_again() -> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Standard).await?;
    world.mount_cdr(&condition_of("p-1")).await;
    let issued = mint(&world.key, &world.issued_claims(Grant::Standard, 2));
    world
        .mount_token_endpoint(Grant::Standard, &issued, 2)
        .await;
    let token = mint(&world.key, &world.person_claims());

    let _first = read_once(&world, &token).await?;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let _second = read_once(&world, &token).await?;

    // ferrohealth-smart renews a margin before the issued token's `exp`, the
    // margin capped at half its remaining life.
    assert_eq!(2, world.onward_requests().await.len());
    Ok(())
}

#[tokio::test]
async fn a_refused_exchange_answers_its_status_and_reaches_no_cdr() -> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Standard).await?;
    world.mount_own_token().await;
    world.mount_cdr(&condition_of("p-1")).await;
    // RFC 8693 §2.2.2: an unacceptable subject token is `invalid_request`.
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .and(matchers::body_string_contains(TOKEN_EXCHANGE))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "invalid_request",
            "error_description": "synthetic: the subject token is not acceptable"
        })))
        .mount(world.issuer.server())
        .await;
    let token = mint(&world.key, &world.person_claims());

    let response = read_once(&world, &token).await?;

    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("OperationOutcome"), "{text}");
    assert!(text.contains("invalid_request"), "{text}");
    assert!(
        !text.contains(&token),
        "the caller's token is in the answer"
    );
    assert!(
        world.cdr_calls().await.is_empty(),
        "no call reached the CDR"
    );
    assert!(
        world
            .token_requests()
            .await
            .iter()
            .all(|request| !request.contains("grant_type=client_credentials")),
        "the bridge's own token is never fetched in place of the exchange"
    );
    Ok(())
}

#[tokio::test]
async fn an_issued_token_for_another_audience_is_never_sent() -> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Standard).await?;
    world.mount_cdr(&condition_of("p-1")).await;
    let mut claims = world.issued_claims(Grant::Standard, 300);
    claims["aud"] = serde_json::json!("https://other.example/api");
    let issued = mint(&world.key, &claims);
    world
        .mount_token_endpoint(Grant::Standard, &issued, 300)
        .await;
    let token = mint(&world.key, &world.person_claims());

    let response = read_once(&world, &token).await?;

    assert_eq!(StatusCode::BAD_GATEWAY, response.status());
    assert!(
        world.cdr_calls().await.is_empty(),
        "no call reached the CDR"
    );
    Ok(())
}

#[tokio::test]
async fn an_issued_token_naming_another_client_is_never_sent() -> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Standard).await?;
    world.mount_cdr(&condition_of("p-1")).await;
    let mut claims = world.issued_claims(Grant::Standard, 300);
    claims["client_id"] = serde_json::json!("synthetic-other-client");
    let issued = mint(&world.key, &claims);
    world
        .mount_token_endpoint(Grant::Standard, &issued, 300)
        .await;
    let token = mint(&world.key, &world.person_claims());

    let response = read_once(&world, &token).await?;

    assert_eq!(StatusCode::BAD_GATEWAY, response.status());
    assert!(
        world.cdr_calls().await.is_empty(),
        "no call reached the CDR"
    );
    Ok(())
}

#[tokio::test]
async fn a_machine_is_never_exchanged_and_reaches_the_cdr_under_the_bridge_token()
-> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Standard).await?;
    world.mount_own_token().await;
    world.mount_cdr(&condition_of("p-1")).await;
    let token = mint(
        &world.key,
        &world.issuer.machine_claims("system/Condition.cru"),
    );

    create_and_read(&world, &token).await?;

    assert!(
        world.onward_requests().await.is_empty(),
        "a machine was exchanged"
    );
    assert_every_call_under(&world.cdr_calls().await, OWN_TOKEN);
    Ok(())
}

/// Creates a Condition as a machine under `grant` and returns the composition
/// the CDR received and the client credentials request the bridge sent.
async fn machine_create(grant: Grant) -> Result<(serde_json::Value, String), Box<dyn Error>> {
    let world = World::start(grant).await?;
    world.mount_own_token().await;
    world.mount_cdr(&condition_of("p-1")).await;
    let token = mint(
        &world.key,
        &world.issuer.machine_claims("system/Condition.cru"),
    );
    let authority = authority(world.issuer.settings()).await;
    let created = raw(
        secured(&world.harness, &authority),
        bearing(post_condition(&condition_of("p-1")), &token),
    )
    .await?;
    assert_eq!(StatusCode::CREATED, created.status(), "the create");
    assert!(
        world.onward_requests().await.is_empty(),
        "a machine was exchanged"
    );
    assert_every_call_under(&world.cdr_calls().await, OWN_TOKEN);
    let commit = world
        .harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|request| {
            request.method.as_str() == "POST" && request.url.path().ends_with("/composition")
        })
        .ok_or("the composition reached the CDR")?;
    let own = world
        .token_requests()
        .await
        .into_iter()
        .find(|body| body.contains("grant_type=client_credentials"))
        .ok_or("the bridge fetched its own token")?;
    Ok((serde_json::from_slice(&commit.body)?, own))
}

#[tokio::test]
async fn a_machine_s_composition_names_its_client_as_the_originating_system()
-> Result<(), Box<dyn Error>> {
    for grant in [Grant::Standard, Grant::Entra] {
        let (composition, own) = machine_create(grant).await?;
        // RM FEEDER_AUDIT: `originating_system_audit` is the audit "from the originating
        // system", `feeder_system_audit` that of the feeder "if different".
        assert_eq!(
            Some(super::CLIENT),
            composition
                .pointer("/feeder_audit/originating_system_audit/system_id")
                .and_then(serde_json::Value::as_str),
            "{composition}"
        );
        assert_eq!(
            Some("ferrobridge.test"),
            composition
                .pointer("/feeder_audit/feeder_system_audit/system_id")
                .and_then(serde_json::Value::as_str),
            "{composition}"
        );
        // RFC 8707 §2: the bridge's own token is asked for the CDR alone.
        assert!(
            own.contains(&format!("resource={}", encoded(CDR_AUDIENCE))),
            "{own}"
        );
    }
    Ok(())
}
