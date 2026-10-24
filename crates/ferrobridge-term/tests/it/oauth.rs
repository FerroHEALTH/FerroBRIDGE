// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OAuth credentials: a provider's token travels as an RFC 6750 bearer
//! token, a refused token is handed back and the request re-sent once, and a
//! provider that obtains no token is a typed error carrying its failure.

use crate::support::{config, release_path};
use ferrobridge_term::client::Client;
use ferrobridge_term::config::{Credentials, TokenFailure, TokenProvider, WireVersion};
use ferrobridge_term::error::Error;
use ferrobridge_term::outcome::LookupOutcome;
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::stubs::terminology;
use http::StatusCode;
use secrecy::{ExposeSecret, SecretString};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The `$lookup` route under the R4 prefix.
fn route() -> String {
    format!("/{}/CodeSystem/$lookup", release_path(WireVersion::R4))
}

/// The failure a scripted provider reports.
#[derive(Debug, thiserror::Error)]
#[error("the synthetic authorization server refused the client")]
struct Refused;

/// A provider that hands out its scripted answers in order and records every
/// token handed back to it.
#[derive(Debug)]
struct Scripted {
    answers: Mutex<VecDeque<Result<&'static str, ()>>>,
    invalidated: Mutex<Vec<String>>,
}

impl Scripted {
    fn new(answers: impl IntoIterator<Item = Result<&'static str, ()>>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.into_iter().collect()),
            invalidated: Mutex::new(Vec::new()),
        })
    }

    fn invalidated(&self) -> Vec<String> {
        self.invalidated.lock().expect("not poisoned").clone()
    }

    fn remaining(&self) -> usize {
        self.answers.lock().expect("not poisoned").len()
    }
}

#[async_trait::async_trait]
impl TokenProvider for Scripted {
    async fn token(&self) -> Result<SecretString, TokenFailure> {
        match self.answers.lock().expect("not poisoned").pop_front() {
            Some(Ok(token)) => Ok(SecretString::from(token)),
            Some(Err(())) | None => Err(TokenFailure::new(Arc::new(Refused))),
        }
    }

    fn invalidate(&self, stale: &SecretString) {
        self.invalidated
            .lock()
            .expect("not poisoned")
            .push(stale.expose_secret().to_owned());
    }
}

/// Returns a client for `server` presenting the tokens `provider` hands out.
fn client(server: &MockServer, provider: Arc<Scripted>) -> Result<Client, Box<dyn std::error::Error>> {
    Ok(Client::new(
        config(server, WireVersion::R4)?.with_credentials(Credentials::OAuth(provider)),
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

/// Mounts the lookup answer for a request carrying `authorization`.
async fn mount_answer(server: &MockServer, authorization: &str) {
    Mock::given(method("POST"))
        .and(path(route()))
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
        .and(path(route()))
        .and(header("authorization", authorization))
        .respond_with(
            ResponseTemplate::new(401)
                .insert_header("WWW-Authenticate", "Bearer error=\"invalid_token\""),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn the_provider_token_travels_as_a_bearer_token() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount_answer(&server, "Bearer synthetic-token-1").await;
    let provider = Scripted::new([Ok("synthetic-token-1")]);

    let outcome = lookup(&client(&server, Arc::clone(&provider))?).await?;

    assert!(
        matches!(outcome, LookupOutcome::Found { .. }),
        "{outcome:?}"
    );
    assert!(provider.invalidated().is_empty(), "no token was refused");
    Ok(())
}

#[tokio::test]
async fn a_refused_token_is_handed_back_and_the_request_sent_once_more()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount_refusal(&server, "Bearer synthetic-token-1").await;
    mount_answer(&server, "Bearer synthetic-token-2").await;
    let provider = Scripted::new([Ok("synthetic-token-1"), Ok("synthetic-token-2")]);

    let outcome = lookup(&client(&server, Arc::clone(&provider))?).await?;

    assert!(
        matches!(outcome, LookupOutcome::Found { .. }),
        "{outcome:?}"
    );
    assert_eq!(
        vec![String::from("synthetic-token-1")],
        provider.invalidated()
    );
    assert_eq!(
        2,
        server.received_requests().await.ok_or("recording")?.len()
    );
    Ok(())
}

#[tokio::test]
async fn a_second_refusal_stands_and_is_not_renewed_again() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount_refusal(&server, "Bearer synthetic-token-1").await;
    mount_refusal(&server, "Bearer synthetic-token-2").await;
    let provider = Scripted::new([
        Ok("synthetic-token-1"),
        Ok("synthetic-token-2"),
        Ok("synthetic-token-3"),
    ]);

    let error = lookup(&client(&server, Arc::clone(&provider))?)
        .await
        .expect_err("the renewed token was refused too");

    let Error::Unauthorized { challenge, .. } = &error else {
        return Err(format!("expected an unauthorized error, got {error:?}").into());
    };
    assert_eq!(Some("Bearer error=\"invalid_token\""), challenge.as_deref());
    assert_eq!(
        2,
        server.received_requests().await.ok_or("recording")?.len()
    );
    assert_eq!(1, provider.remaining(), "one renewal per request");
    assert!(!error.is_retryable());
    Ok(())
}

#[tokio::test]
async fn a_renewal_that_obtains_no_token_keeps_the_refusal_and_the_cause()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount_refusal(&server, "Bearer synthetic-token-1").await;
    let provider = Scripted::new([Ok("synthetic-token-1"), Err(())]);

    let error = lookup(&client(&server, Arc::clone(&provider))?)
        .await
        .expect_err("no new token could be obtained");

    assert!(matches!(error, Error::Renewal { .. }), "{error:?}");
    assert_eq!("token-renewal", error.kind());
    assert_eq!(Some(StatusCode::UNAUTHORIZED), error.upstream_status());
    let failure = std::error::Error::source(&error).ok_or("the renewal carries its cause")?;
    assert!(failure.is::<TokenFailure>(), "{failure:?}");
    assert!(
        failure.source().is_some_and(<dyn std::error::Error>::is::<Refused>),
        "the provider's own error is the root of the chain"
    );
    assert!(!error.is_retryable());
    assert_eq!(
        1,
        server.received_requests().await.ok_or("recording")?.len()
    );
    Ok(())
}

#[tokio::test]
async fn a_provider_that_obtains_no_token_sends_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let provider = Scripted::new([Err(())]);

    let error = lookup(&client(&server, provider)?)
        .await
        .expect_err("no token could be obtained");

    assert!(matches!(error, Error::Token { .. }), "{error:?}");
    assert_eq!("token", error.kind());
    assert_eq!(None, error.upstream_status());
    assert!(!error.is_retryable());
    assert!(
        server
            .received_requests()
            .await
            .ok_or("recording")?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn a_probe_renews_a_refused_token_once() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let metadata = format!("/{}/metadata", release_path(WireVersion::R4));
    Mock::given(method("GET"))
        .and(path(metadata.as_str()))
        .and(header("authorization", "Bearer synthetic-token-1"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(metadata.as_str()))
        .and(header("authorization", "Bearer synthetic-token-2"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let provider = Scripted::new([Ok("synthetic-token-1"), Ok("synthetic-token-2")]);

    let status = client(&server, Arc::clone(&provider))?
        .reachability()
        .await?;

    assert_eq!(StatusCode::OK, status);
    assert_eq!(
        vec![String::from("synthetic-token-1")],
        provider.invalidated()
    );
    Ok(())
}
