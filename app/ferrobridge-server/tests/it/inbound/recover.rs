// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Redelivery and reconciliation against a stub source: a lost event is
//! caught by the since-cursor poll, the redelivery runs at start and after a
//! run of `503` answers, and one replica leads a source's recovery, its
//! former leader refused a cursor write. The cases run only when
//! `FERROBRIDGE_E2E=1` admits the harness.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrobridge_operator_api::sources::{Kind, Write};
use ferrobridge_server::inbound::event::Change;
use ferrobridge_server::inbound::intake::{Intake, routes};
use ferrobridge_server::inbound::recover::Recoverer;
use ferrobridge_server::inbound::store::{CursorError, InboxStore};
use ferrobridge_testkit::containers;
use serde_json::json;
use tower::ServiceExt as _;
use wiremock::matchers::{method, path, query_param_contains};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::store::{create, replica, sources};

/// A webhook source whose recovery calls `stub`: a redelivery after two
/// `503` answers, and the updates and deletes polls every hour.
pub(crate) fn recovering(name: &str, stub: &str) -> Write {
    Write {
        name: name.to_owned(),
        kind: Kind::Webhook,
        enabled: true,
        document: json!({
            "verify": {"kind": "static-header", "header": "X-Key"},
            "keys": [{"secret": "webhook-key"}],
            "events": {
                "tenant": "/customerCode", "entity": "/id", "otherwise": "upsert",
                "dedupe": {"pointers": ["/customerCode", "/id", "/timestamp"]}
            },
            "tenants": [{"tenant": "t1", "identifier_system": "urn:example:t1"}],
            "limits": {"max_pending": 1},
            "recovery": {
                "redeliver": {"method": "PUT", "url": format!("{stub}/webhooks/redeliver"), "after_unavailable": 2},
                "reconcile": {
                    "every": 3600, "overlap": 60, "first": 86400,
                    "polls": [
                        {"url": format!("{stub}/v0/xstream/clients/updates"), "cursor": "since", "items": "/clients", "entity": "/id", "change": "upsert"},
                        {"url": format!("{stub}/v0/xstream/clients/deletes"), "cursor": "since", "items": "/clients", "entity": "/id", "change": "remove"}
                    ]
                }
            }
        }),
    }
}

/// Answers the two polls of [`recovering`] on `stub`, each once.
pub(crate) async fn polls(stub: &MockServer, updated: &[u32], deleted: &[u32]) {
    for (route, ids) in [
        ("/v0/xstream/clients/updates", updated),
        ("/v0/xstream/clients/deletes", deleted),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .and(query_param_contains("since", "T"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "clients": ids.iter().map(|id| json!({"id": id})).collect::<Vec<_>>()
            })))
            .expect(1)
            .mount(stub)
            .await;
    }
}

#[tokio::test]
async fn a_reconciliation_catches_a_patient_whose_event_was_lost() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let stub = MockServer::start().await;
    let source = create(&held, &recovering("ons", &stub.uri())).await?;
    polls(&stub, &[42], &[7]).await;
    let recoverer = Recoverer::new(store.clone(), Arc::clone(&held), Duration::from_secs(30));
    let lease = recoverer.lease_of(source.id)?;
    let generation = lease.claim().await?.ok_or("the lease is free")?;

    assert_eq!(
        2,
        recoverer
            .reconcile(&source, &lease, generation, false)
            .await?
    );
    let inbox = InboxStore::new(store.pool().clone());
    let mut claimed = inbox
        .claim(uuid::Uuid::new_v4(), Duration::from_secs(30), 10)
        .await?;
    claimed.sort_by(|a, b| a.key.entity.cmp(&b.key.entity));
    assert_eq!(
        vec![("42", Change::Upsert), ("7", Change::Remove)],
        claimed
            .iter()
            .map(|item| (item.key.entity.as_str(), item.change))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        0,
        recoverer
            .reconcile(&source, &lease, generation, false)
            .await?,
        "a cursor polled within the hour waits"
    );
    stub.verify().await;
    Ok(())
}

#[tokio::test]
async fn the_redelivery_runs_at_start_and_after_a_run_of_503() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let stub = MockServer::start().await;
    create(&held, &recovering("ons", &stub.uri())).await?;
    Mock::given(method("PUT"))
        .and(path("/webhooks/redeliver"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&stub)
        .await;
    polls(&stub, &[], &[]).await;

    let recoverer = Arc::new(Recoverer::new(
        store.clone(),
        Arc::clone(&held),
        Duration::from_secs(30),
    ));
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let round = recoverer.spawn(Duration::from_secs(3600), &stopped);

    let inbox = InboxStore::new(store.pool().clone());
    let app = crate::inbound::intake::app_with(routes(Arc::new(
        Intake::new(Arc::clone(&held), inbox.clone(), 1024).with_recoverer(Arc::clone(&recoverer)),
    )));
    let send = |id: u32| {
        http::Request::post("/inbound/ons")
            .header("X-Key", "synthetic-connector-secret")
            .body(axum::body::Body::from(
                json!({"customerCode": "t1", "id": id, "timestamp": "x"}).to_string(),
            ))
    };
    assert_eq!(
        http::StatusCode::OK,
        app.clone().oneshot(send(1)?).await?.status()
    );
    for id in [2, 3] {
        assert_eq!(
            http::StatusCode::SERVICE_UNAVAILABLE,
            app.clone().oneshot(send(id)?).await?.status()
        );
    }
    let worker = uuid::Uuid::new_v4();
    for item in inbox.claim(worker, Duration::from_secs(30), 10).await? {
        inbox
            .give_back(
                &item,
                worker,
                ferrobridge_server::inbound::store::Returned::Done,
            )
            .await?;
    }
    assert_eq!(http::StatusCode::OK, app.oneshot(send(4)?).await?.status());

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while stub
        .received_requests()
        .await
        .ok_or("the stub records its requests")?
        .iter()
        .filter(|request| request.method == wiremock::http::Method::PUT)
        .count()
        < 2
        && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    stop.send_replace(true);
    round.await?;
    stub.verify().await;
    Ok(())
}

#[tokio::test]
async fn one_replica_leads_and_a_former_leader_cannot_move_a_cursor() -> Result<(), Box<dyn Error>>
{
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let (one, two) = (
        replica(postgres.url()).await?,
        replica(postgres.url()).await?,
    );
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&one, secrets.path())?);
    let stub = MockServer::start().await;
    let source = create(&held, &recovering("ons", &stub.uri())).await?;
    let (first, second) = (
        Recoverer::new(one.clone(), Arc::clone(&held), Duration::from_millis(200)),
        Recoverer::new(two.clone(), Arc::clone(&held), Duration::from_millis(200)),
    );
    let (lease_one, lease_two) = (first.lease_of(source.id)?, second.lease_of(source.id)?);
    let former = lease_one.claim().await?.ok_or("the lease is free")?;
    assert!(
        lease_two.claim().await?.is_none(),
        "a second replica does not lead while the first holds the lease"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    let current = lease_two.claim().await?.ok_or("the lease ran out")?;
    let inbox = InboxStore::new(one.pool().clone());
    let refused = inbox
        .move_cursor(
            &lease_one,
            former,
            source.id,
            "t1",
            0,
            jiff::Timestamp::now(),
        )
        .await;
    assert!(
        matches!(refused, Err(CursorError::Fenced(_))),
        "{refused:?}"
    );
    InboxStore::new(two.pool().clone())
        .move_cursor(
            &lease_two,
            current,
            source.id,
            "t1",
            0,
            jiff::Timestamp::now(),
        )
        .await?;
    assert!(inbox.cursor(source.id, "t1", 0).await?.is_some());
    Ok(())
}
