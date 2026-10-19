// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology credential the bridge obtains with the OAuth 2.0 client
//! credentials grant, through the token cache the CDR lane uses: the token
//! fetch, the cache across calls, and the one renewal after a terminology
//! `401`, from `[terminology.credentials.oauth]` to the wire.

use ferrobridge_server::cdr::oauth::TokenError;
use ferrobridge_server::config::Config;
use ferrobridge_term::client::Client;
use ferrobridge_term::error::Error;
use ferrobridge_term::outcome::LookupOutcome;
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::stubs::terminology;
use http::StatusCode;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// The token endpoint path every stub serves.
const TOKEN_PATH: &str = "/oauth/token";

/// The `$lookup` route under the configured base.
const LOOKUP_PATH: &str = "/r4/CodeSystem/$lookup";

/// Returns the terminology client a `[terminology.credentials.oauth]`
/// section against `authorization` resolves to, for the server `terminology`.
fn client(
    terminology: &MockServer,
    authorization: &MockServer,
) -> Result<Client, Box<dyn StdError>> {
    let text = format!(
        concat!(
            "[terminology]\nbase_url = \"{}/r4\"\n\n",
            "[terminology.retry]\nmax_attempts = 1\n\n",
            "[terminology.credentials.oauth]\n",
            "token_endpoint = \"{}{}\"\n",
            "client_id = \"ferrobridge\"\n",
            "client_secret = \"synthetic-secret\"\n",
            "audience = \"ferroterm\"\n",
        ),
        terminology.uri(),
        authorization.uri(),
        TOKEN_PATH,
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let config = settings.terminology.ok_or("the terminology lane is on")?;
    Ok(Client::new(config)?)
}

/// Returns an access token response for `token` living five minutes.
fn issued(token: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "access_token": token,
        "token_type": "Bearer",
        "expires_in": 300,
    }))
}

/// Mounts `answer` on the token endpoint for its next request only.
async fn mount_token_once(server: &MockServer, answer: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(answer)
        .up_to_n_times(1)
        .mount(server)
        .await;
}

/// Mounts the lookup answer for a request carrying `authorization`.
async fn mount_answer(server: &MockServer, authorization: &str) {
    Mock::given(method("POST"))
        .and(path(LOOKUP_PATH))
        .and(header("authorization", authorization))
        .respond_with(terminology::lookup(
            "FerroBridgeSyntheticSource",
            fixtures::TERM_MEMBER_DISPLAY,
            None,
        ))
        .mount(server)
        .await;
}

/// Mounts a `401` for a request carrying `authorization`.
async fn mount_refusal(server: &MockServer, authorization: &str) {
    Mock::given(method("POST"))
        .and(path(LOOKUP_PATH))
        .and(header("authorization", authorization))
        .respond_with(ResponseTemplate::new(401))
        .mount(server)
        .await;
}

/// Sends the fixture lookup to `client`.
async fn lookup(client: &Client) -> Result<LookupOutcome, Error> {
    client
        .lookup(
            fixtures::TERM_SOURCE_SYSTEM,
            fixtures::TERM_MEMBER_CODE,
            None,
        )
        .await
}

/// Returns how many requests `server` received on `route`.
async fn received(server: &MockServer, route: &str) -> Result<usize, Box<dyn StdError>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("the mock server is recording")?
        .iter()
        .filter(|request: &&Request| request.url.path() == route)
        .count())
}

#[tokio::test]
async fn the_token_is_fetched_with_the_grant_and_sent_as_a_bearer_token()
-> Result<(), Box<dyn StdError>> {
    let authorization = MockServer::start().await;
    let terminology_server = MockServer::start().await;
    // NOTE: RFC 6749 §4.4.2, the request names the grant; §2.3.1, the client
    // authenticates with HTTP Basic by default.
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .and(body_string_contains("grant_type=client_credentials"))
        .and(body_string_contains("audience=ferroterm"))
        .and(header(
            "authorization",
            "Basic ZmVycm9icmlkZ2U6c3ludGhldGljLXNlY3JldA==",
        ))
        .respond_with(issued("synthetic-token-1"))
        .expect(1)
        .mount(&authorization)
        .await;
    mount_answer(&terminology_server, "Bearer synthetic-token-1").await;

    let outcome = lookup(&client(&terminology_server, &authorization)?).await?;

    assert!(
        matches!(outcome, LookupOutcome::Found { .. }),
        "{outcome:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_token_is_cached_across_calls() -> Result<(), Box<dyn StdError>> {
    let authorization = MockServer::start().await;
    let terminology_server = MockServer::start().await;
    mount_token_once(&authorization, issued("synthetic-token-1")).await;
    mount_answer(&terminology_server, "Bearer synthetic-token-1").await;
    let client = client(&terminology_server, &authorization)?;

    lookup(&client).await?;
    lookup(&client).await?;
    client.clone().reachability().await?;

    assert_eq!(1, received(&authorization, TOKEN_PATH).await?);
    assert_eq!(2, received(&terminology_server, LOOKUP_PATH).await?);
    Ok(())
}

#[tokio::test]
async fn a_terminology_401_renews_the_token_once() -> Result<(), Box<dyn StdError>> {
    let authorization = MockServer::start().await;
    let terminology_server = MockServer::start().await;
    mount_token_once(&authorization, issued("synthetic-token-1")).await;
    mount_token_once(&authorization, issued("synthetic-token-2")).await;
    mount_refusal(&terminology_server, "Bearer synthetic-token-1").await;
    mount_answer(&terminology_server, "Bearer synthetic-token-2").await;
    let client = client(&terminology_server, &authorization)?;

    let outcome = lookup(&client).await?;
    lookup(&client).await?;

    assert!(
        matches!(outcome, LookupOutcome::Found { .. }),
        "{outcome:?}"
    );
    assert_eq!(
        2,
        received(&authorization, TOKEN_PATH).await?,
        "the renewed token is cached for the next call"
    );
    assert_eq!(3, received(&terminology_server, LOOKUP_PATH).await?);
    Ok(())
}

#[tokio::test]
async fn a_renewed_token_refused_again_is_not_renewed_twice() -> Result<(), Box<dyn StdError>> {
    let authorization = MockServer::start().await;
    let terminology_server = MockServer::start().await;
    mount_token_once(&authorization, issued("synthetic-token-1")).await;
    mount_token_once(&authorization, issued("synthetic-token-2")).await;
    mount_token_once(&authorization, issued("synthetic-token-3")).await;
    mount_refusal(&terminology_server, "Bearer synthetic-token-1").await;
    mount_refusal(&terminology_server, "Bearer synthetic-token-2").await;

    let error = lookup(&client(&terminology_server, &authorization)?)
        .await
        .expect_err("the renewed token was refused too");

    assert!(matches!(error, Error::Unauthorized { .. }), "{error:?}");
    assert_eq!(2, received(&authorization, TOKEN_PATH).await?);
    assert_eq!(2, received(&terminology_server, LOOKUP_PATH).await?);
    Ok(())
}

#[tokio::test]
async fn a_renewal_the_authorization_server_refuses_carries_its_error()
-> Result<(), Box<dyn StdError>> {
    let authorization = MockServer::start().await;
    let terminology_server = MockServer::start().await;
    mount_token_once(&authorization, issued("synthetic-token-1")).await;
    mount_token_once(
        &authorization,
        ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": "invalid_client",
            "error_description": "unknown client",
        })),
    )
    .await;
    mount_refusal(&terminology_server, "Bearer synthetic-token-1").await;

    let error = lookup(&client(&terminology_server, &authorization)?)
        .await
        .expect_err("no new token could be obtained");

    assert!(matches!(error, Error::Renewal { .. }), "{error:?}");
    assert_eq!(Some(StatusCode::UNAUTHORIZED), error.upstream_status());
    let cause = StdError::source(&error)
        .and_then(StdError::source)
        .ok_or("the renewal carries the token error")?;
    let token_error = cause
        .downcast_ref::<TokenError>()
        .ok_or_else(|| format!("expected a token error, got {cause:?}"))?;
    assert_eq!(Some(StatusCode::UNAUTHORIZED), token_error.status());
    assert!(
        token_error.to_string().contains("invalid_client"),
        "{token_error}"
    );
    assert_eq!(1, received(&terminology_server, LOOKUP_PATH).await?);
    Ok(())
}
