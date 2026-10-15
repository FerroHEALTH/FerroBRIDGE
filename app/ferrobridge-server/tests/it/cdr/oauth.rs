// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The CDR credential the bridge obtains with the OAuth 2.0 client
//! credentials grant: discovery, the token request in both client
//! authentication forms, the cache, the renewal after a refused token, and
//! every refusal of the authorization server as a typed error.

use super::support;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::Prefer;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::cdr::error::CdrError;
use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::cdr::oauth::cache::{Clock, TokenSource};
use ferrobridge_server::cdr::oauth::{ClientAuth, Endpoint, OAuthConfig, RenewalError, TokenError};
use http::StatusCode;
use openehr_its::rest::client::{ClientError, RetryPolicy, TransportError};
use openehr_its::rest::generated::ehr::client::EhrGetByIdOutcome;
use secrecy::SecretString;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// The EHR every CDR case reads.
const EHR: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The token endpoint path every stub serves.
const TOKEN_PATH: &str = "/oauth/token";

/// Returns a grant against the token endpoint of `server`.
fn grant(server: &MockServer) -> Result<OAuthConfig, Box<dyn Error>> {
    Ok(OAuthConfig::new(
        Endpoint::Token(format!("{}{TOKEN_PATH}", server.uri()).parse()?),
        String::from("bridge"),
        SecretString::from("synthetic-secret"),
    ))
}

/// Returns an access token response for `token` living `expires_in` seconds.
fn issued(token: &str, expires_in: u64) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "access_token": token,
        "token_type": "Bearer",
        "expires_in": expires_in,
    }))
}

/// Returns how many requests `server` received on `route`.
async fn received(server: &MockServer, route: &str) -> Result<usize, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("the mock server is recording")?
        .iter()
        .filter(|request: &&Request| request.url.path() == route)
        .count())
}

/// Returns the token error a failed `token()` carries.
fn token_error(error: &Arc<TokenError>) -> &TokenError {
    error
}

/// Returns the token error at the root of a CDR refusal for want of a
/// credential.
fn credential_refusal(error: &CdrError) -> Result<&TokenError, Box<dyn Error>> {
    let CdrError::Client { source, upstream } = error else {
        return Err(format!("expected a refusal of the generated client, got {error:?}").into());
    };
    assert!(upstream.is_none(), "nothing reached the CDR");
    let ClientError::Credentials { source, .. } = source.as_ref() else {
        return Err(format!("expected a credentials refusal, got {source:?}").into());
    };
    Error::source(source)
        .and_then(|cause| cause.downcast_ref::<Arc<TokenError>>())
        .map(AsRef::as_ref)
        .ok_or_else(|| "the refusal carries the token error".into())
}

/// A clock the test moves by hand.
#[derive(Debug)]
struct ManualClock(Mutex<Instant>);

impl ManualClock {
    fn advance(&self, by: Duration) {
        let mut now = self.0.lock().expect("the clock is not poisoned");
        *now += by;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self.0.lock().expect("the clock is not poisoned")
    }
}

#[tokio::test]
async fn the_token_endpoint_is_discovered_from_rfc_8414_metadata() -> Result<(), Box<dyn Error>> {
    // NOTE: RFC 8414 §3.1 inserts the well-known suffix before the issuer's
    // path, and §3.3 requires the document's issuer to be the configured one.
    let server = MockServer::start().await;
    let issuer = format!("{}/realms/ferro", server.uri());
    Mock::given(method("GET"))
        .and(path("/.well-known/oauth-authorization-server/realms/ferro"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issuer": issuer,
            "token_endpoint": format!("{}{TOKEN_PATH}", server.uri()),
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(issued("synthetic-token-1", 300))
        .expect(1)
        .mount(&server)
        .await;

    let config = OAuthConfig::new(
        Endpoint::Issuer(issuer),
        String::from("bridge"),
        SecretString::from("synthetic-secret"),
    );
    let source = TokenSource::new(config, Duration::from_secs(5))?;
    let token = source.token().await.map_err(|error| error.to_string())?;
    assert_eq!("Bearer synthetic-token-1", token.header().to_str()?);
    Ok(())
}

#[tokio::test]
async fn discovery_falls_back_to_the_openid_connect_location() -> Result<(), Box<dyn Error>> {
    // NOTE: OpenID Connect Discovery 1.0 §4 appends the suffix to the issuer;
    // it is read only after the RFC 8414 location answers 404.
    let server = MockServer::start().await;
    let issuer = format!("{}/realms/ferro", server.uri());
    Mock::given(method("GET"))
        .and(path("/realms/ferro/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issuer": issuer,
            "token_endpoint": format!("{}{TOKEN_PATH}", server.uri()),
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(issued("synthetic-token-1", 300))
        .mount(&server)
        .await;

    let config = OAuthConfig::new(
        Endpoint::Issuer(issuer),
        String::from("bridge"),
        SecretString::from("synthetic-secret"),
    );
    let source = TokenSource::new(config, Duration::from_secs(5))?;
    assert!(source.token().await.is_ok());
    Ok(())
}

#[tokio::test]
async fn metadata_naming_another_issuer_is_refused() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/oauth-authorization-server/realms/ferro"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "issuer": "https://attacker.invalid",
            "token_endpoint": format!("{}{TOKEN_PATH}", server.uri()),
        })))
        .mount(&server)
        .await;

    let config = OAuthConfig::new(
        Endpoint::Issuer(format!("{}/realms/ferro", server.uri())),
        String::from("bridge"),
        SecretString::from("synthetic-secret"),
    );
    let error = TokenSource::new(config, Duration::from_secs(5))?
        .token()
        .await
        .expect_err("RFC 8414 §3.3 refuses a mismatched issuer");
    assert!(
        matches!(token_error(&error), TokenError::IssuerMismatch { found, .. } if found == "https://attacker.invalid"),
        "{error:?}"
    );
    assert_eq!(
        0,
        received(&server, TOKEN_PATH).await?,
        "no token was asked"
    );
    Ok(())
}

#[tokio::test]
async fn client_secret_basic_form_encodes_the_id_and_the_secret() -> Result<(), Box<dyn Error>> {
    // NOTE: RFC 6749 §2.3.1, base64 of "bridge+client:s3cr3t%2Fsecret", the
    // id and the secret form-encoded first (Appendix B).
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .and(header(
            "authorization",
            "Basic YnJpZGdlK2NsaWVudDpzM2NyM3QlMkZzZWNyZXQ=",
        ))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string_contains("grant_type=client_credentials"))
        .and(body_string_contains("scope=openehr+system%2F*.crs"))
        .and(body_string_contains("audience=ferroehr"))
        .and(body_string_contains(
            "resource=https%3A%2F%2Fcdr.example.org%2Fv1",
        ))
        .respond_with(issued("synthetic-token-1", 300))
        .expect(1)
        .mount(&server)
        .await;

    let mut config = OAuthConfig::new(
        Endpoint::Token(format!("{}{TOKEN_PATH}", server.uri()).parse()?),
        String::from("bridge client"),
        SecretString::from("s3cr3t/secret"),
    );
    config.scope = Some(String::from("openehr system/*.crs"));
    config.audience = Some(String::from("ferroehr"));
    config.resource = Some("https://cdr.example.org/v1".parse()?);
    let source = TokenSource::new(config, Duration::from_secs(5))?;
    assert!(source.token().await.is_ok());
    let requests = server.received_requests().await.ok_or("recording")?;
    let body = String::from_utf8(requests.first().ok_or("one request")?.body.clone())?;
    assert!(
        !body.contains("client_secret"),
        "the secret is not in the body"
    );
    Ok(())
}

#[tokio::test]
async fn client_secret_post_carries_the_id_and_the_secret_in_the_body() -> Result<(), Box<dyn Error>>
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .and(body_string_contains("grant_type=client_credentials"))
        .and(body_string_contains("client_id=bridge"))
        .and(body_string_contains("client_secret=synthetic-secret"))
        .respond_with(issued("synthetic-token-1", 300))
        .expect(1)
        .mount(&server)
        .await;

    let mut config = grant(&server)?;
    config.client_auth = ClientAuth::ClientSecretPost;
    assert!(
        TokenSource::new(config, Duration::from_secs(5))?
            .token()
            .await
            .is_ok()
    );
    let requests = server.received_requests().await.ok_or("recording")?;
    let request = requests.first().ok_or("one request")?;
    assert!(
        !request.headers.contains_key("authorization"),
        "client_secret_post sends no Authorization header"
    );
    Ok(())
}

#[tokio::test]
async fn a_cached_token_is_reused() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(issued("synthetic-token-1", 300))
        .expect(1)
        .mount(&server)
        .await;

    let source = TokenSource::new(grant(&server)?, Duration::from_secs(5))?;
    for _ in 0..3 {
        let token = source.token().await.map_err(|error| error.to_string())?;
        assert_eq!("Bearer synthetic-token-1", token.header().to_str()?);
        assert!(!format!("{token:?}").contains("synthetic-token-1"));
    }
    let rendered = format!("{source:?}");
    assert!(
        !rendered.contains("synthetic-token-1") && !rendered.contains("synthetic-secret"),
        "the Debug rendering of the cache carries neither the token nor the secret"
    );
    Ok(())
}

#[tokio::test]
async fn a_token_is_renewed_the_margin_before_it_expires() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(issued("synthetic-token-1", 300))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(issued("synthetic-token-2", 300))
        .mount(&server)
        .await;

    let clock = Arc::new(ManualClock(Mutex::new(Instant::now())));
    let source = TokenSource::with_clock(grant(&server)?, Duration::from_secs(5), clock.clone())?;
    let header = |token: Arc<ferrobridge_server::cdr::oauth::cache::Token>| {
        token.header().to_str().map(str::to_owned)
    };
    let first = source.token().await.map_err(|error| error.to_string())?;
    assert_eq!("Bearer synthetic-token-1", header(first)?);

    clock.advance(Duration::from_secs(269));
    let still = source.token().await.map_err(|error| error.to_string())?;
    assert_eq!(
        "Bearer synthetic-token-1",
        header(still)?,
        "within 300 - 30 seconds"
    );

    clock.advance(Duration::from_secs(2));
    let renewed = source.token().await.map_err(|error| error.to_string())?;
    assert_eq!("Bearer synthetic-token-2", header(renewed)?);
    assert_eq!(2, received(&server, TOKEN_PATH).await?);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_callers_share_one_fetch() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(issued("synthetic-token-1", 300).set_delay(Duration::from_millis(200)))
        .mount(&server)
        .await;

    let source = TokenSource::new(grant(&server)?, Duration::from_secs(5))?;
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let source = source.clone();
        tasks.push(tokio::spawn(async move { source.token().await }));
    }
    for task in tasks {
        let token = task.await?.map_err(|error| error.to_string())?;
        assert_eq!("Bearer synthetic-token-1", token.header().to_str()?);
    }
    assert_eq!(1, received(&server, TOKEN_PATH).await?);
    Ok(())
}

#[tokio::test]
async fn a_refused_grant_is_a_typed_error_with_the_oauth_error() -> Result<(), Box<dyn Error>> {
    // NOTE: RFC 6749 §5.2, a refused client answers `invalid_client`.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "invalid_client",
            "error_description": "unknown client",
        })))
        .mount(&server)
        .await;

    let client = CdrClient::new(
        &CdrConfig::new(format!("{}/v1", server.uri()).parse()?).with_oauth(grant(&server)?),
    )?;
    let error = client
        .ehr(&EhrId::new(EHR)?)
        .await
        .expect_err("no token, no call");
    match credential_refusal(&error)? {
        TokenError::Refused {
            status,
            error: Some(oauth),
            ..
        } => {
            assert_eq!(StatusCode::BAD_REQUEST, *status);
            assert_eq!("invalid_client", oauth.error);
            assert_eq!(Some("unknown client"), oauth.error_description.as_deref());
        }
        other => return Err(format!("expected the OAuth refusal, got {other:?}").into()),
    }
    assert!(!format!("{error:?}").contains("synthetic-secret"));
    assert_eq!(0, received(&server, &format!("/v1/ehr/{EHR}")).await?);
    Ok(())
}

#[tokio::test]
async fn a_failing_token_endpoint_keeps_its_status() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(503).set_body_string("restarting"))
        .mount(&server)
        .await;

    let error = TokenSource::new(grant(&server)?, Duration::from_secs(5))?
        .token()
        .await
        .expect_err("a 503 is no token");
    assert!(
        matches!(token_error(&error), TokenError::Refused { status, error: None, body } if *status == StatusCode::SERVICE_UNAVAILABLE && body == "restarting"),
        "{error:?}"
    );
    assert_eq!(Some(StatusCode::SERVICE_UNAVAILABLE), error.status());
    Ok(())
}

#[tokio::test]
async fn a_failed_fetch_is_not_cached() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(issued("synthetic-token-1", 300))
        .mount(&server)
        .await;

    let source = TokenSource::new(grant(&server)?, Duration::from_secs(5))?;
    assert!(source.token().await.is_err());
    assert!(source.token().await.is_ok(), "the next call fetches again");
    Ok(())
}

#[tokio::test]
async fn a_malformed_token_response_is_a_typed_error() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"token_type": "Bearer"})),
        )
        .mount(&server)
        .await;

    let error = TokenSource::new(grant(&server)?, Duration::from_secs(5))?
        .token()
        .await
        .expect_err("RFC 6749 §5.1 requires access_token");
    assert!(
        matches!(token_error(&error), TokenError::Malformed { .. }),
        "{error:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_token_that_is_not_a_bearer_token_is_refused() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "synthetic-token-1",
            "token_type": "DPoP",
        })))
        .mount(&server)
        .await;

    let error = TokenSource::new(grant(&server)?, Duration::from_secs(5))?
        .token()
        .await
        .expect_err("the bridge sends bearer tokens");
    assert!(
        matches!(token_error(&error), TokenError::TokenType { found } if found == "DPoP"),
        "{error:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_token_type_is_compared_case_insensitively() -> Result<(), Box<dyn Error>> {
    // NOTE: RFC 6749 §5.1, the `token_type` value is case insensitive.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "synthetic-token-1",
            "token_type": "bearer",
        })))
        .mount(&server)
        .await;

    let source = TokenSource::new(grant(&server)?, Duration::from_secs(5))?;
    assert!(source.token().await.is_ok());
    Ok(())
}

#[tokio::test]
async fn every_cdr_call_carries_the_token() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(issued("synthetic-token-1", 300))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR}")))
        .and(header("authorization", "Bearer synthetic-token-1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(support::EHR_JSON))
        .expect(2)
        .mount(&server)
        .await;

    let client = CdrClient::new(
        &CdrConfig::new(format!("{}/v1", server.uri()).parse()?).with_oauth(grant(&server)?),
    )?;
    for _ in 0..2 {
        let answered = client.ehr(&EhrId::new(EHR)?).await?;
        assert!(matches!(answered.outcome, EhrGetByIdOutcome::Ok { .. }));
    }
    Ok(())
}

#[tokio::test]
async fn a_refused_token_is_renewed_and_the_call_sent_once_more() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(issued("synthetic-token-1", 300))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(issued("synthetic-token-2", 300))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/ehr"))
        .and(header("authorization", "Bearer synthetic-token-1"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/ehr"))
        .and(header("authorization", "Bearer synthetic-token-2"))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("ETag", format!("\"{EHR}\""))
                .insert_header("Location", format!("{}/v1/ehr/{EHR}", server.uri())),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = CdrClient::new(
        &CdrConfig::new(format!("{}/v1", server.uri()).parse()?).with_oauth(grant(&server)?),
    )?;
    let answered = client.create_ehr(None, Prefer::Minimal).await?;
    assert_eq!(StatusCode::CREATED, answered.upstream.status());
    assert_eq!(2, received(&server, TOKEN_PATH).await?);
    Ok(())
}

#[tokio::test]
async fn a_second_refusal_reaches_the_caller() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(issued("synthetic-token-1", 300))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR}")))
        .respond_with(ResponseTemplate::new(401).insert_header("WWW-Authenticate", "Bearer"))
        .expect(2)
        .mount(&server)
        .await;

    let client = CdrClient::new(
        &CdrConfig::new(format!("{}/v1", server.uri()).parse()?)
            .with_retry(RetryPolicy {
                max_attempts: 3,
                initial_backoff: Duration::from_millis(1),
                max_backoff: Duration::from_millis(2),
            })
            .with_oauth(grant(&server)?),
    )?;
    let error = client
        .ehr(&EhrId::new(EHR)?)
        .await
        .expect_err("a token refused twice is refused");
    let CdrError::Client { source, upstream } = error else {
        return Err(format!("expected a refusal of the generated client, got {error:?}").into());
    };
    assert!(
        matches!(source.as_ref(), ClientError::Unauthorized { .. }),
        "{source:?}"
    );
    assert_eq!(
        Some(StatusCode::UNAUTHORIZED),
        upstream.map(|upstream| upstream.status())
    );
    assert_eq!(2, received(&server, TOKEN_PATH).await?, "one renewal");
    Ok(())
}

#[tokio::test]
async fn a_renewal_that_fails_after_a_refusal_carries_the_token_error() -> Result<(), Box<dyn Error>>
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(issued("synthetic-token-1", 300))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "invalid_client",
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/ehr"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;

    let client = CdrClient::new(
        &CdrConfig::new(format!("{}/v1", server.uri()).parse()?).with_oauth(grant(&server)?),
    )?;
    let error = client
        .create_ehr(None, Prefer::Minimal)
        .await
        .expect_err("no renewed token, no second send");
    let CdrError::Client { source, .. } = &error else {
        return Err(format!("expected a refusal of the generated client, got {error:?}").into());
    };
    let ClientError::Transport {
        source: TransportError::Send { source },
        ..
    } = source.as_ref()
    else {
        return Err(format!("expected the renewal failure, got {source:?}").into());
    };
    let renewal = source
        .downcast_ref::<RenewalError>()
        .ok_or("the transport refusal is the renewal failure")?;
    assert!(
        matches!(renewal.token_error(), TokenError::Refused { error: Some(oauth), .. } if oauth.error == "invalid_client"),
        "{renewal:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_token_that_cannot_be_obtained_makes_the_probe_fail() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": "invalid_client",
        })))
        .mount(&server)
        .await;

    let client = CdrClient::new(
        &CdrConfig::new(format!("{}/v1", server.uri()).parse()?).with_oauth(grant(&server)?),
    )?;
    let error = client
        .reachability()
        .await
        .expect_err("a probe with no token never reaches the CDR");
    assert!(
        matches!(credential_refusal(&error)?, TokenError::Refused { status, .. } if *status == StatusCode::UNAUTHORIZED),
        "{error:?}"
    );
    Ok(())
}
