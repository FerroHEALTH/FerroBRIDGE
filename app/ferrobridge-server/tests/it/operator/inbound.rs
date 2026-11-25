// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `/operator/sources/{id}/parked`, `…/settle`, `…/replay`, `…/reconcile`
//! and `…/redeliver` over a state store: a parked item is listed with its
//! reason, settled with the operator's subject and reason, work runs again
//! by entity or by time range, and every write logs one operator audit line.
//! The cases run only when `FERROBRIDGE_E2E=1` admits the harness.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrobridge_operator_api::inbound::{ParkedItems, Replayed};
use ferrobridge_server::inbound::event::{EventMap, Reading};
use ferrobridge_server::inbound::intake::Intake;
use ferrobridge_server::inbound::recover::Recoverer;
use ferrobridge_server::inbound::store::{InboxStore, Returned};
use ferrobridge_testkit::containers;
use http::{HeaderMap, Method, StatusCode};
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{Case, OPERATOR, bytes};
use crate::inbound::recover::{polls, recovering};
use crate::inbound::store::{create, replica, sources, webhook};
use crate::support::{Logs, subscriber};

/// Returns a subscriber that writes the JSON console to `logs`.
fn capture(logs: &Logs) -> impl tracing::Subscriber + Send + Sync {
    subscriber(
        ferrohealth_telemetry::section::Rendering::Json,
        "info",
        false,
        logs.clone(),
    )
}

/// Asserts `logs` holds exactly one operator audit line of `action` on
/// `source` with `status`, naming the caller and no value of the event.
fn assert_audited(logs: &Logs, action: &str, source: Uuid, status: u16) {
    let text = logs.text();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|line| line["target"] == "ferrobridge::audit" && line["action"] == action)
        .collect();
    assert_eq!(1, lines.len(), "{text}");
    assert_eq!("synthetic-client-sub", lines[0]["subject"], "{text}");
    assert_eq!(source.to_string(), lines[0]["source"], "{text}");
    assert_eq!(status, lines[0]["status"], "{text}");
    assert!(lines[0].get("entity").is_none(), "{text}");
    assert!(lines[0].get("reason").is_none(), "{text}");
}

/// The event of tenant `t1` about entity `id`.
fn event(id: u32) -> ferrobridge_server::inbound::event::Event {
    let map: EventMap = serde_json::from_value(json!({
        "tenant": "/customerCode", "entity": "/id", "otherwise": "upsert",
        "dedupe": {"pointers": ["/id"]}
    }))
    .expect("the map");
    match map
        .read(&HeaderMap::new(), &json!({"customerCode": "t1", "id": id}))
        .expect("a reading")
    {
        Reading::Store(event) => Some(event),
        Reading::Answer(_) => None,
    }
    .expect("a stored event")
}

/// A case over a state store whose source `ons` has two parked items, with
/// the container and the secrets directory kept for it.
struct Parked {
    case: Case,
    store: ferrobridge_server::store::Store,
    inbox: InboxStore,
    source: Uuid,
    _kept: (containers::Postgres, tempfile::TempDir),
}

/// Returns the case, `None` unless `FERROBRIDGE_E2E=1`.
async fn parked_case() -> Result<Option<Parked>, Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(None);
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let source = create(&held, &webhook("ons", &["t1"], 4)).await?;
    let inbox = InboxStore::new(store.pool().clone());
    let case = Case::intake(
        Arc::clone(&held),
        Arc::new(Intake::new(held, inbox.clone(), 1024)),
    )
    .await?;
    for id in [7, 8] {
        inbox.accept(source.id, "t1", &event(id)).await?;
    }
    let worker = Uuid::new_v4();
    for item in inbox.claim(worker, Duration::from_secs(30), 10).await? {
        inbox
            .give_back(
                &item,
                worker,
                Returned::Park("the identity source refused the patient"),
            )
            .await?;
    }
    Ok(Some(Parked {
        case,
        store,
        inbox,
        source: source.id,
        _kept: (postgres, secrets),
    }))
}

#[tokio::test]
async fn a_parked_item_is_listed_and_settled() -> Result<(), Box<dyn Error>> {
    let Some(Parked {
        case,
        store,
        inbox,
        source,
        _kept: _container,
    }) = parked_case().await?
    else {
        return Ok(());
    };
    let listed = case
        .get(
            &format!("/operator/sources/{source}/parked"),
            Some(OPERATOR),
        )
        .await?;
    assert_eq!(StatusCode::OK, listed.status());
    let parked: ParkedItems = serde_json::from_slice(&bytes(listed).await?)?;
    assert_eq!(2, parked.items.len());
    assert_eq!(
        "the identity source refused the patient",
        parked.items[0].reason
    );

    let settle = format!("/operator/sources/{source}/settle");
    let unreasoned = case
        .send(
            Method::POST,
            &settle,
            OPERATOR,
            None,
            Some(&json!({"tenant": "t1", "entity": "7", "settlement": "retry", "reason": " "})),
        )
        .await?;
    assert_eq!(StatusCode::BAD_REQUEST, unreasoned.status());
    let retried = case
        .send(
            Method::POST,
            &settle,
            OPERATOR,
            None,
            Some(&json!({"tenant": "t1", "entity": "7", "settlement": "retry", "reason": "corrected at the source"})),
        )
        .await?;
    assert_eq!(StatusCode::NO_CONTENT, retried.status());
    let discarded = case
        .send(
            Method::POST,
            &settle,
            OPERATOR,
            None,
            Some(&json!({"tenant": "t1", "entity": "8", "settlement": "discard", "reason": "a test record"})),
        )
        .await?;
    assert_eq!(StatusCode::NO_CONTENT, discarded.status());
    let again = case
        .send(
            Method::POST,
            &settle,
            OPERATOR,
            None,
            Some(&json!({"tenant": "t1", "entity": "8", "settlement": "retry", "reason": "again"})),
        )
        .await?;
    assert_eq!(StatusCode::NOT_FOUND, again.status());
    let (by, why): (String, String) = sqlx::query_as(
        "SELECT settled_by, settled_reason FROM inbound_work WHERE entity_key = '7'",
    )
    .fetch_one(store.pool())
    .await?;
    assert_ne!("-", by, "the operator's subject is recorded");
    assert_eq!("corrected at the source", why);
    assert_eq!(1, inbox.pending(source).await?);

    Ok(())
}

#[tokio::test]
async fn work_is_replayed_by_entity_and_a_backwards_range_is_refused() -> Result<(), Box<dyn Error>>
{
    let Some(Parked {
        case,
        inbox,
        source,
        _kept: _container,
        ..
    }) = parked_case().await?
    else {
        return Ok(());
    };
    let replay = format!("/operator/sources/{source}/replay");
    let one = case
        .send(
            Method::POST,
            &replay,
            OPERATOR,
            None,
            Some(&json!({"tenant": "t1", "entity": "8"})),
        )
        .await?;
    assert_eq!(StatusCode::ACCEPTED, one.status());
    let replayed: Replayed = serde_json::from_slice(&bytes(one).await?)?;
    assert_eq!(1, replayed.items);
    assert_eq!(
        1,
        inbox.pending(source).await?,
        "the replayed parked item waits again"
    );

    let backwards = case
        .send(
            Method::POST,
            &replay,
            OPERATOR,
            None,
            Some(&json!({"from": "2026-11-24T11:00:00Z", "to": "2026-11-24T10:00:00Z"})),
        )
        .await?;
    assert_eq!(StatusCode::BAD_REQUEST, backwards.status());
    let unknown = case
        .get(
            &format!("/operator/sources/{}/parked", Uuid::new_v4()),
            Some(OPERATOR),
        )
        .await?;
    assert_eq!(StatusCode::NOT_FOUND, unknown.status());
    Ok(())
}

#[tokio::test]
async fn a_settlement_and_a_replay_each_log_one_audit_line() -> Result<(), Box<dyn Error>> {
    let logs = Logs::default();
    let _logging = tracing::subscriber::set_default(capture(&logs));
    let Some(Parked {
        case,
        source,
        _kept: _container,
        ..
    }) = parked_case().await?
    else {
        return Ok(());
    };
    for (entity, settlement, reason) in [
        ("7", "retry", "corrected at the source"),
        ("8", "discard", "a test record"),
    ] {
        let settled = case
            .send(
                Method::POST,
                &format!("/operator/sources/{source}/settle"),
                OPERATOR,
                None,
                Some(&json!({"tenant": "t1", "entity": entity, "settlement": settlement, "reason": reason})),
            )
            .await?;
        assert_eq!(StatusCode::NO_CONTENT, settled.status());
        assert_audited(&logs, settlement, source, 204);
    }
    let replayed = case
        .send(
            Method::POST,
            &format!("/operator/sources/{source}/replay"),
            OPERATOR,
            None,
            Some(&json!({"tenant": "t1", "entity": "8"})),
        )
        .await?;
    assert_eq!(StatusCode::ACCEPTED, replayed.status());
    assert_audited(&logs, "replay", source, 202);
    Ok(())
}

#[tokio::test]
async fn a_reconciliation_and_a_redelivery_each_log_one_audit_line() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let logs = Logs::default();
    let _logging = tracing::subscriber::set_default(capture(&logs));
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let stub = MockServer::start().await;
    let source = create(&held, &recovering("ons", &stub.uri())).await?;
    polls(&stub, &[42], &[]).await;
    Mock::given(method("PUT"))
        .and(path("/webhooks/redeliver"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&stub)
        .await;
    let recoverer = Arc::new(Recoverer::new(
        store.clone(),
        Arc::clone(&held),
        Duration::from_secs(30),
    ));
    let inbox = InboxStore::new(store.pool().clone());
    let case = Case::intake(
        Arc::clone(&held),
        Arc::new(Intake::new(held, inbox, 1024).with_recoverer(recoverer)),
    )
    .await?;

    let reconciled = case
        .post(
            &format!("/operator/sources/{}/reconcile", source.id),
            OPERATOR,
        )
        .await?;
    assert_eq!(StatusCode::ACCEPTED, reconciled.status());
    assert_audited(&logs, "reconcile", source.id, 202);
    let redelivered = case
        .post(
            &format!("/operator/sources/{}/redeliver", source.id),
            OPERATOR,
        )
        .await?;
    assert_eq!(StatusCode::NO_CONTENT, redelivered.status());
    assert_audited(&logs, "redeliver", source.id, 204);
    stub.verify().await;
    Ok(())
}
