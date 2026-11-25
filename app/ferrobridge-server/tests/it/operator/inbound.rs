// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `/operator/sources/{id}/parked`, `…/settle` and `…/replay` over a state
//! store: a parked item is listed with its reason, settled with the
//! operator's subject and reason, and work runs again by entity or by time
//! range. The cases run only when `FERROBRIDGE_E2E=1` admits the harness.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrobridge_operator_api::inbound::{ParkedItems, Replayed};
use ferrobridge_server::inbound::event::{EventMap, Reading};
use ferrobridge_server::inbound::intake::Intake;
use ferrobridge_server::inbound::store::{InboxStore, Returned};
use ferrobridge_testkit::containers;
use http::{HeaderMap, Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

use super::{Case, OPERATOR, bytes};
use crate::inbound::store::{create, replica, sources, webhook};

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
