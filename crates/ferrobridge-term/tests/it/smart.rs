// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The client credentials token of `ferrohealth-smart` as the provider: the
//! cached token travels as the bearer token, and a token the server refuses
//! is dropped, so the one repeated request carries a token fetched anew.

use crate::support::{config, release_path};
use ferrobridge_term::client::Client;
use ferrobridge_term::config::{Credentials, WireVersion};
use ferrobridge_term::error::Error;
use ferrobridge_term::outcome::LookupOutcome;
use ferrobridge_term::smart::SmartToken;
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::stubs::terminology;
use ferrohealth_smart::client::cache::TokenSource;
use ferrohealth_smart::client::{ClientCredentials, Endpoint};
use secrecy::SecretString;
use std::sync::Arc;
use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A token response of RFC 6749 §5.1 carrying the synthetic `token`.
fn token_response(token: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "access_token": token,
        "token_type": "Bearer",
        "expires_in": 300
    }))
}

/// A client of `server` whose provider is a grant at `server`'s `/token`.
fn client(server: &MockServer) -> Result<Client, Box<dyn std::error::Error>> {
    let grant = ClientCredentials::new(
        Endpoint::Token(Url::parse(&format!("{}/token", server.uri()))?),
        String::from("synthetic-bridge"),
        SecretString::from("synthetic-secret"),
    );
    let provider = SmartToken::from(TokenSource::new(grant)?);
    Ok(Client::new(
        config(server, WireVersion::R4)?.with_credentials(Credentials::OAuth(Arc::new(provider))),
    )?)
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

#[tokio::test]
async fn a_refused_token_is_fetched_anew_and_the_request_sent_once_more()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(token_response("synthetic-first"))
        .up_to_n_times(1)
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(token_response("synthetic-second"))
        .with_priority(2)
        .expect(1)
        .mount(&server)
        .await;
    let route = format!("/{}/CodeSystem/$lookup", release_path(WireVersion::R4));
    Mock::given(method("POST"))
        .and(path(route.as_str()))
        .and(header("authorization", "Bearer synthetic-first"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(route.as_str()))
        .and(header("authorization", "Bearer synthetic-second"))
        .respond_with(terminology::lookup(
            "FerroBridgeSyntheticSource",
            fixtures::TERM_MEMBER_DISPLAY,
            None,
        ))
        .expect(1)
        .mount(&server)
        .await;

    let outcome = lookup(&client(&server)?).await?;

    assert!(
        matches!(outcome, LookupOutcome::Found { .. }),
        "{outcome:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_grant_is_a_typed_error_and_sends_nothing()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(serde_json::json!({ "error": "invalid_client" })),
        )
        .mount(&server)
        .await;

    let outcome = lookup(&client(&server)?).await;

    assert!(outcome.is_err(), "{outcome:?}");
    let sent = server.received_requests().await.ok_or("recording")?;
    assert!(
        sent.iter().all(|request| request.url.path() == "/token"),
        "no terminology request was sent"
    );
    Ok(())
}
