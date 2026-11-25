// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `POST /inbound/{source}` through the shipped stack: a verified event is
//! stored before it is answered, a wrong signature stores nothing, a probe
//! and an unserved tenant are answered unstored, and back-pressure and a
//! store that is down answer `503`. The cases that need the state store run
//! only when `FERROBRIDGE_E2E=1` admits the harness.

use std::error::Error;
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use aws_lc_rs::hmac;
use axum::body::Body;
use ferrobridge_server::inbound::intake::{Intake, routes};
use ferrobridge_server::inbound::store::InboxStore;
use ferrobridge_server::sources::Sources;
use ferrobridge_server::with_middleware;
use ferrobridge_testkit::containers;
use http::{Request, StatusCode, header};
use serde_json::json;
use tower::ServiceExt as _;

use super::store::{create, replica, sources, webhook};
use crate::support;

/// The connector secret the source's key file holds.
const SECRET: &[u8] = b"synthetic-connector-secret";

/// The lowercase hex HMAC-SHA-512 of `body` under [`SECRET`].
fn signature(body: &[u8]) -> String {
    let tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA512, SECRET), body);
    tag.as_ref().iter().fold(String::new(), |mut text, byte| {
        write!(text, "{byte:02x}").expect("writing to a string");
        text
    })
}

/// A notification of tenant `tenant` about entity `id`.
fn notification(tenant: &str, event_type: &str, id: u32) -> Vec<u8> {
    json!({"customerCode": tenant, "eventType": event_type, "id": id, "timestamp": "2026-11-24T10:00:00Z"})
        .to_string()
        .into_bytes()
}

/// `body` posted to `source`, signed with `signature`.
fn post(source: &str, body: Vec<u8>, signature: &str) -> Result<Request<Body>, http::Error> {
    Request::post(format!("/inbound/{source}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Signature-SHA512", signature)
        .body(Body::from(body))
}

/// The intake over `sources` and the store behind `pool`, in the shipped
/// stack.
fn app(sources: Arc<Sources>, pool: sqlx::postgres::PgPool) -> axum::Router {
    with_middleware(
        routes(Arc::new(Intake::new(
            sources,
            InboxStore::new(pool),
            1024 * 1024,
        ))),
        support::state(),
        &support::settings(),
    )
}

#[tokio::test]
async fn a_verified_event_is_stored_and_a_repeat_changes_nothing() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let source = create(&held, &webhook("ons", &["t1"], 4)).await?;
    let inbox = InboxStore::new(store.pool().clone());
    let app = app(Arc::clone(&held), store.pool().clone());

    let body = notification("t1", "UPDATE", 7);
    let signed = signature(&body);
    for _ in 0..2 {
        let answer = app
            .clone()
            .oneshot(post("ons", body.clone(), &signed)?)
            .await?;
        assert_eq!(StatusCode::OK, answer.status());
    }
    assert_eq!(1, inbox.pending(source.id).await?);
    let claimed = inbox
        .claim(uuid::Uuid::new_v4(), Duration::from_secs(30), 10)
        .await?;
    assert_eq!(1, claimed[0].generation, "the repeat stored nothing");
    Ok(())
}

#[tokio::test]
async fn a_wrong_signature_answers_401_and_stores_nothing() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let source = create(&held, &webhook("ons", &["t1"], 4)).await?;
    let inbox = InboxStore::new(store.pool().clone());
    let app = app(Arc::clone(&held), store.pool().clone());

    let body = notification("t1", "UPDATE", 7);
    let forged = signature(&notification("t1", "UPDATE", 8));
    let answer = app.clone().oneshot(post("ons", body, &forged)?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, answer.status());
    let problem: serde_json::Value =
        serde_json::from_slice(&axum::body::to_bytes(answer.into_body(), 64 * 1024).await?)?;
    assert!(
        problem["type"]
            .as_str()
            .is_some_and(|kind| kind.ends_with("#unverified")),
        "{problem}"
    );
    assert_eq!(0, inbox.pending(source.id).await?);
    Ok(())
}

#[tokio::test]
async fn a_probe_an_ignored_event_and_an_unserved_tenant_are_answered_unstored()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let source = create(&held, &webhook("ons", &["t1"], 4)).await?;
    let inbox = InboxStore::new(store.pool().clone());
    let app = app(Arc::clone(&held), store.pool().clone());

    for body in [
        notification("t1", "NOP", 0),
        notification("t1", "ARCHIVE", 7),
        notification("t9", "UPDATE", 7),
    ] {
        let signed = signature(&body);
        let answer = app.clone().oneshot(post("ons", body, &signed)?).await?;
        assert_eq!(StatusCode::OK, answer.status());
    }
    assert_eq!(0, inbox.pending(source.id).await?);
    Ok(())
}

#[tokio::test]
async fn an_unknown_source_answers_404_and_an_unreadable_event_400() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    create(&held, &webhook("ons", &["t1"], 4)).await?;
    let app = app(Arc::clone(&held), store.pool().clone());

    let body = notification("t1", "UPDATE", 7);
    let signed = signature(&body);
    let unknown = app.clone().oneshot(post("other", body, &signed)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, unknown.status());

    let no_entity = br#"{"customerCode":"t1","eventType":"UPDATE","timestamp":"x"}"#.to_vec();
    let signed = signature(&no_entity);
    let unreadable = app
        .clone()
        .oneshot(post("ons", no_entity, &signed)?)
        .await?;
    assert_eq!(StatusCode::BAD_REQUEST, unreadable.status());

    let not_json = b"not json".to_vec();
    let signed = signature(&not_json);
    let refused = app.oneshot(post("ons", not_json, &signed)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, refused.status());
    Ok(())
}

#[tokio::test]
async fn work_above_max_pending_answers_503() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let source = create(&held, &webhook("ons", &["t1"], 4)).await?;
    let inbox = InboxStore::new(store.pool().clone());
    let app = app(Arc::clone(&held), store.pool().clone());

    for id in 0..3 {
        let body = notification("t1", "UPDATE", id);
        let signed = signature(&body);
        let answer = app.clone().oneshot(post("ons", body, &signed)?).await?;
        assert_eq!(StatusCode::OK, answer.status());
    }
    let body = notification("t1", "UPDATE", 3);
    let signed = signature(&body);
    let answer = app.oneshot(post("ons", body, &signed)?).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, answer.status());
    assert!(answer.headers().contains_key(header::RETRY_AFTER));
    assert_eq!(3, inbox.pending(source.id).await?);
    Ok(())
}

#[tokio::test]
async fn a_store_that_is_down_answers_503() -> Result<(), Box<dyn Error>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(200))
        .connect_lazy("postgres://nobody:nothing@127.0.0.1:1/none?sslmode=disable")?;
    let app = app(Arc::new(Sources::new(pool.clone(), None)), pool);
    let body = notification("t1", "UPDATE", 7);
    let signed = signature(&body);
    let answer = app.oneshot(post("ons", body, &signed)?).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, answer.status());
    assert!(answer.headers().contains_key(header::RETRY_AFTER));
    Ok(())
}
