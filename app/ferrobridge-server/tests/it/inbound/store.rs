// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbox, the work items and the slot pool over a PostgreSQL 19
//! container, with two replicas' stores on one database. The cases run only
//! when `FERROBRIDGE_E2E=1` admits the harness.

use std::collections::BTreeSet;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ferrobridge_operator_api::sources::{Kind, Write};
use ferrobridge_server::inbound::event::{Change, EventMap, Reading};
use ferrobridge_server::inbound::store::{
    Accepted, Claimed, InboxStore, Outcome, Replay, Returned, WorkKey,
};
use ferrobridge_server::inbound::worker::{Attempt, Failed, Handler, Pool, Settings};
use ferrobridge_server::retry::{Backoff, Decided, Decision, Failure, Policy};
use ferrobridge_server::sources::{Source, Sources};
use ferrobridge_server::store::Store;
use ferrobridge_testkit::containers;
use http::HeaderMap;
use serde_json::{Value, json};
use uuid::Uuid;

/// A replica's store on the database at `url`.
pub(crate) async fn replica(url: &str) -> Result<Store, Box<dyn Error>> {
    Ok(Store::open(&ferrobridge_server::store::Settings {
        connection: omop_cdm::connection::CdmConnection::new(url, None)?,
        max_connections: 8,
        acquire_timeout: Duration::from_secs(30),
        seal: None,
    })
    .await?)
}

/// A webhook source whose events name their tenant, entity and stamp, each
/// tenant of `tenants` served, verified by HMAC under the secret
/// `webhook-key` and keyed over the whole event.
pub(crate) fn webhook(name: &str, tenants: &[&str], concurrency: u32) -> Write {
    Write {
        name: name.to_owned(),
        kind: Kind::Webhook,
        enabled: true,
        document: json!({
            "verify": {"kind": "hmac", "algorithm": "sha512", "header": "X-Signature-SHA512"},
            "keys": [{"secret": "webhook-key"}],
            "events": map_document(),
            "tenants": tenants.iter().map(|tenant| json!({
                "tenant": tenant,
                "identifier_system": format!("urn:example:{tenant}")
            })).collect::<Vec<_>>(),
            "limits": {"concurrency": concurrency, "max_pending": 3}
        }),
    }
}

/// The event map of [`webhook`].
fn map_document() -> Value {
    json!({
        "tenant": "/customerCode",
        "entity": "/id",
        "rules": [
            {"when": {"/eventType": "CREATE"}, "action": "upsert"},
            {"when": {"/eventType": "UPDATE"}, "action": "upsert"},
            {"when": {"/eventType": "DELETE"}, "action": "remove"},
            {"when": {"/eventType": "NOP"}, "action": "probe"}
        ],
        "dedupe": {"pointers": ["/customerCode", "/eventType", "/id", "/timestamp"]},
        "stamp": "/timestamp"
    })
}

/// The sources of `store`, resolving secrets from `secrets`.
pub(crate) fn sources(store: &Store, secrets: &std::path::Path) -> Result<Sources, Box<dyn Error>> {
    std::fs::write(secrets.join("webhook-key"), "synthetic-connector-secret")?;
    Ok(Sources::open(
        store,
        Some(&ferrohealth_secret::dir::SecretDirConfig {
            dir: secrets.to_path_buf(),
            poll_seconds: ferrohealth_secret::dir::PollSeconds::default(),
        }),
    )?)
}

/// Creates `write` through `sources`.
pub(crate) async fn create(sources: &Sources, write: &Write) -> Result<Source, Box<dyn Error>> {
    let document = sources.validate(write.kind, &write.document)?;
    Ok(sources
        .store()
        .create(write, &document, "operator-test")
        .await?)
}

/// The event of tenant `tenant` about entity `id`.
fn event(
    tenant: &str,
    event_type: &str,
    id: u32,
    timestamp: &str,
) -> ferrobridge_server::inbound::event::Event {
    let map: EventMap = serde_json::from_value(map_document()).expect("the map");
    let body =
        json!({"customerCode": tenant, "eventType": event_type, "id": id, "timestamp": timestamp});
    match map.read(&HeaderMap::new(), &body).expect("a reading") {
        Reading::Store(event) => Some(event),
        Reading::Answer(_) => None,
    }
    .expect("a stored event")
}

const LEASE: Duration = Duration::from_secs(30);

#[tokio::test]
async fn an_event_is_stored_once_and_coalesces_per_entity() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let source = create(
        &sources(&store, secrets.path())?,
        &webhook("ons", &["t1"], 4),
    )
    .await?;
    let inbox = InboxStore::new(store.pool().clone());

    let update = event("t1", "UPDATE", 7, "2026-11-24T10:00:01Z");
    let create_ = event("t1", "CREATE", 7, "2026-11-24T10:00:00Z");
    assert_eq!(
        Accepted::Stored,
        inbox.accept(source.id, "t1", &update).await?
    );
    assert_eq!(
        Accepted::Repeated,
        inbox.accept(source.id, "t1", &update).await?
    );
    assert_eq!(
        Accepted::Stored,
        inbox.accept(source.id, "t1", &create_).await?
    );
    assert_eq!(1, inbox.pending(source.id).await?, "one item per entity");

    let claimed = inbox.claim(Uuid::new_v4(), LEASE, 10).await?;
    assert_eq!(1, claimed.len());
    assert_eq!(2, claimed[0].generation, "two distinct events, one repeat");
    assert_eq!(Change::Upsert, claimed[0].change);
    Ok(())
}

#[tokio::test]
async fn an_event_during_the_work_runs_the_item_again() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let source = create(
        &sources(&store, secrets.path())?,
        &webhook("ons", &["t1"], 4),
    )
    .await?;
    let inbox = InboxStore::new(store.pool().clone());
    let worker = Uuid::new_v4();

    inbox
        .accept(source.id, "t1", &event("t1", "UPDATE", 7, "a"))
        .await?;
    let first = inbox.claim(worker, LEASE, 1).await?.remove(0);
    inbox
        .accept(source.id, "t1", &event("t1", "UPDATE", 7, "b"))
        .await?;
    assert!(
        inbox.claim(Uuid::new_v4(), LEASE, 1).await?.is_empty(),
        "a running item is claimed by no one else"
    );
    assert_eq!(
        Outcome::Rerun,
        inbox.give_back(&first, worker, Returned::Done).await?
    );
    let second = inbox.claim(worker, LEASE, 1).await?.remove(0);
    assert_eq!(first.generation + 1, second.generation);
    assert_eq!(
        Outcome::Settled,
        inbox.give_back(&second, worker, Returned::Done).await?
    );
    assert_eq!(0, inbox.pending(source.id).await?);
    Ok(())
}

#[tokio::test]
async fn a_lost_lease_refuses_the_former_holder() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let source = create(
        &sources(&store, secrets.path())?,
        &webhook("ons", &["t1"], 4),
    )
    .await?;
    let inbox = InboxStore::new(store.pool().clone());
    inbox
        .accept(source.id, "t1", &event("t1", "UPDATE", 7, "a"))
        .await?;
    let (former, current) = (Uuid::new_v4(), Uuid::new_v4());
    let stale = inbox
        .claim(former, Duration::from_millis(1), 1)
        .await?
        .remove(0);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let taken = inbox.claim(current, LEASE, 1).await?.remove(0);
    assert_eq!(
        Outcome::Lost,
        inbox.give_back(&stale, former, Returned::Done).await?
    );
    assert_eq!(
        Outcome::Settled,
        inbox.give_back(&taken, current, Returned::Done).await?
    );
    Ok(())
}

#[tokio::test]
async fn the_slot_pool_holds_its_limit_across_two_replicas() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let (one, two) = (
        replica(postgres.url()).await?,
        replica(postgres.url()).await?,
    );
    let secrets = tempfile::tempdir()?;
    let source = create(&sources(&one, secrets.path())?, &webhook("ons", &["t1"], 4)).await?;
    let (first, second) = (
        InboxStore::new(one.pool().clone()),
        InboxStore::new(two.pool().clone()),
    );
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let mut held = Vec::new();
    for (inbox, holder) in [(&first, a), (&second, b), (&first, a), (&second, b)] {
        held.push(
            inbox
                .take_slot(source.id, "t1", 4, holder, LEASE)
                .await?
                .ok_or("a free slot")?,
        );
    }
    assert_eq!(
        BTreeSet::from([0, 1, 2, 3]),
        held.iter().copied().collect::<BTreeSet<_>>()
    );
    assert_eq!(None, first.take_slot(source.id, "t1", 4, a, LEASE).await?);
    assert_eq!(None, second.take_slot(source.id, "t1", 4, b, LEASE).await?);
    assert!(
        first
            .take_slot(source.id, "t2", 4, a, LEASE)
            .await?
            .is_some(),
        "each tenant has its own pool"
    );
    second.release_slot(source.id, "t1", held[1], b).await?;
    assert_eq!(
        Some(held[1]),
        first.take_slot(source.id, "t1", 4, a, LEASE).await?
    );
    Ok(())
}

/// A handler that records the entities in flight and fails when two
/// workers hold one entity at once.
#[derive(Debug, Default)]
struct Recording {
    in_flight: Mutex<BTreeSet<String>>,
    applied: Mutex<Vec<String>>,
    overlapped: Mutex<bool>,
}

impl Handler for Recording {
    fn apply<'a>(&'a self, _source: &'a Source, item: &'a Claimed) -> Attempt<'a> {
        Box::pin(async move {
            let fresh = self
                .in_flight
                .lock()
                .expect("the lock")
                .insert(item.key.entity.clone());
            if !fresh {
                *self.overlapped.lock().expect("the lock") = true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            self.in_flight
                .lock()
                .expect("the lock")
                .remove(&item.key.entity);
            self.applied
                .lock()
                .expect("the lock")
                .push(item.key.entity.clone());
            Ok(())
        })
    }
}

/// The settlement `decision` of the test operator, for `reason`.
fn decided(decision: Decision, reason: &str) -> Decided {
    Decided {
        decision,
        by: String::from("operator-test"),
        reason: reason.to_owned(),
    }
}

const POLICY: Policy = Policy {
    backoff: Backoff {
        initial: Duration::from_millis(10),
        max: Duration::from_millis(40),
    },
    max_attempts: 2,
};

fn settings(workers: u32) -> Settings {
    Settings {
        workers,
        lease: LEASE,
        policy: POLICY,
        idle: Duration::from_millis(20),
    }
}

#[tokio::test]
async fn two_replicas_workers_never_hold_one_entity_at_once() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let (one, two) = (
        replica(postgres.url()).await?,
        replica(postgres.url()).await?,
    );
    let secrets = tempfile::tempdir()?;
    let (first_sources, second_sources) = (
        Arc::new(sources(&one, secrets.path())?),
        Arc::new(sources(&two, secrets.path())?),
    );
    let source = create(&first_sources, &webhook("ons", &["t1"], 8)).await?;
    let inbox = InboxStore::new(one.pool().clone());
    for id in 0..12 {
        inbox
            .accept(
                source.id,
                "t1",
                &event("t1", "UPDATE", id % 4, &format!("s{id}")),
            )
            .await?;
    }
    let handler = Arc::new(Recording::default());
    let pools = [
        Arc::new(Pool::new(
            InboxStore::new(one.pool().clone()),
            first_sources,
            handler.clone(),
            settings(3),
        )),
        Arc::new(Pool::new(
            InboxStore::new(two.pool().clone()),
            second_sources,
            handler.clone(),
            settings(3),
        )),
    ];
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let workers: Vec<_> = pools.iter().flat_map(|pool| pool.spawn(&stopped)).collect();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while inbox.pending(source.id).await? > 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    stop.send_replace(true);
    for worker in workers {
        worker.await?;
    }
    assert_eq!(0, inbox.pending(source.id).await?, "every item is done");
    assert!(!*handler.overlapped.lock().expect("the lock"));
    let applied: BTreeSet<_> = handler
        .applied
        .lock()
        .expect("the lock")
        .iter()
        .cloned()
        .collect();
    assert_eq!(
        BTreeSet::from(["0", "1", "2", "3"].map(String::from)),
        applied
    );
    Ok(())
}

/// A handler that fails every attempt as `failure`.
#[derive(Debug)]
struct Failing(Failure);

impl Handler for Failing {
    fn apply<'a>(&'a self, _source: &'a Source, _item: &'a Claimed) -> Attempt<'a> {
        let failure = self.0;
        Box::pin(async move {
            Err(Failed {
                failure,
                reason: "the identity source refused the patient",
            })
        })
    }
}

#[tokio::test]
async fn a_failure_retries_parks_and_an_operator_settles_it() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let source = create(&held, &webhook("ons", &["t1"], 4)).await?;
    let inbox = InboxStore::new(store.pool().clone());
    inbox
        .accept(source.id, "t1", &event("t1", "UPDATE", 7, "a"))
        .await?;
    inbox
        .accept(source.id, "t1", &event("t1", "UPDATE", 8, "a"))
        .await?;

    let transient = Pool::new(
        inbox.clone(),
        Arc::clone(&held),
        Arc::new(Failing(Failure::Transient)),
        settings(1),
    );
    assert!(transient.step().await?, "the first attempt runs");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while inbox.parked(source.id).await?.len() < 2 && tokio::time::Instant::now() < deadline {
        transient.step().await?;
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    let parked = inbox.parked(source.id).await?;
    assert_eq!(2, parked.len(), "both items park after their attempts");
    assert_eq!("the identity source refused the patient", parked[0].reason);
    assert_eq!(2, parked[0].attempts);

    let retried = WorkKey {
        source_id: source.id,
        tenant: String::from("t1"),
        entity: String::from("7"),
    };
    assert!(
        inbox
            .settle(
                &retried,
                &decided(Decision::Retry, "the patient was corrected")
            )
            .await?
    );
    let discarded = WorkKey {
        entity: String::from("8"),
        ..retried.clone()
    };
    assert!(
        inbox
            .settle(&discarded, &decided(Decision::Discard, "a test record"))
            .await?
    );
    assert!(
        !inbox
            .settle(&discarded, &decided(Decision::Retry, "again"))
            .await?,
        "a settled item is no longer parked"
    );
    assert_eq!(
        1,
        inbox.pending(source.id).await?,
        "only the retried item runs"
    );
    Ok(())
}

#[tokio::test]
async fn a_refusal_parks_at_once() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let held = Arc::new(sources(&store, secrets.path())?);
    let source = create(&held, &webhook("ons", &["t1"], 4)).await?;
    let inbox = InboxStore::new(store.pool().clone());
    inbox
        .accept(source.id, "t1", &event("t1", "UPDATE", 7, "a"))
        .await?;
    let refusing = Pool::new(
        inbox.clone(),
        held,
        Arc::new(Failing(Failure::Poisoned)),
        settings(1),
    );
    assert!(refusing.step().await?);
    let parked = inbox.parked(source.id).await?;
    assert_eq!(1, parked.len());
    assert_eq!(1, parked[0].attempts);
    Ok(())
}

#[tokio::test]
async fn a_replay_runs_an_entity_or_a_range_again() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let source = create(
        &sources(&store, secrets.path())?,
        &webhook("ons", &["t1"], 4),
    )
    .await?;
    let inbox = InboxStore::new(store.pool().clone());
    let worker = Uuid::new_v4();
    let before = jiff::Timestamp::now();
    for id in [1, 2] {
        inbox
            .accept(source.id, "t1", &event("t1", "UPDATE", id, "a"))
            .await?;
    }
    for item in inbox.claim(worker, LEASE, 10).await? {
        inbox.give_back(&item, worker, Returned::Done).await?;
    }
    assert_eq!(0, inbox.pending(source.id).await?);

    let one = Replay::Entity {
        tenant: String::from("t1"),
        entity: String::from("1"),
    };
    assert_eq!(1, inbox.replay(source.id, &one).await?);
    assert_eq!(1, inbox.pending(source.id).await?);
    let range = Replay::Received {
        from: before,
        to: jiff::Timestamp::now() + jiff::SignedDuration::from_secs(1),
    };
    assert_eq!(2, inbox.replay(source.id, &range).await?);
    assert_eq!(2, inbox.pending(source.id).await?);
    let empty = Replay::Received {
        from: before - jiff::SignedDuration::from_hours(2),
        to: before - jiff::SignedDuration::from_hours(1),
    };
    assert_eq!(0, inbox.replay(source.id, &empty).await?);
    Ok(())
}

#[tokio::test]
async fn a_claim_takes_no_more_than_its_limit() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = replica(postgres.url()).await?;
    let secrets = tempfile::tempdir()?;
    let source = create(
        &sources(&store, secrets.path())?,
        &webhook("ons", &["t1"], 4),
    )
    .await?;
    let inbox = InboxStore::new(store.pool().clone());
    for id in 0..3 {
        inbox
            .accept(source.id, "t1", &event("t1", "UPDATE", id, "a"))
            .await?;
    }
    assert_eq!(1, inbox.claim(Uuid::new_v4(), LEASE, 1).await?.len());
    assert_eq!(2, inbox.claim(Uuid::new_v4(), LEASE, 5).await?.len());
    Ok(())
}
