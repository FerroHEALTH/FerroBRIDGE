// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An event that carries its record: the body sealed under `[state.seal]`,
//! read for the generation its worker applies, cleared once applied or
//! discarded (GDPR Art 5(1)(c) and (e)), a repeat changing nothing and an
//! older stamp superseded. The cases run only when `FERROBRIDGE_E2E=1`
//! admits the harness.

use std::error::Error;
use std::time::Duration;

use ferrobridge_operator_api::sources::{Kind, Write};
use ferrobridge_server::inbound::event::{Change, DedupeKey, Event};
use ferrobridge_server::inbound::store::{Accepted, Body, InboxStore, Returned, WorkKey};
use ferrobridge_server::retry::{Decided, Decision};
use serde_json::json;
use uuid::Uuid;

use super::store::{create, sources};

/// The identifier system an MLLP patient event is stored under.
const SYSTEM: &str = "http://example.org/fhir/sid/northhosp-mrn";

/// How long a claim is held.
const LEASE: Duration = Duration::from_secs(30);

/// The MLLP source of the cases.
fn mllp() -> Write {
    Write {
        name: String::from("northhosp-adt"),
        kind: Kind::Mllp,
        enabled: true,
        document: json!({"routes": [{
            "sending_application": "ADMIT", "sending_facility": "NORTHHOSP",
            "source_cidrs": ["127.0.0.0/8"]
        }]}),
    }
}

/// The upsert of `entity` with control id `control`, stamped `stamp`.
fn event(entity: &str, control: &str, stamp: &str) -> Event {
    Event {
        change: Change::Upsert,
        tenant: Some(SYSTEM.to_owned()),
        entity: entity.to_owned(),
        dedupe_key: DedupeKey::of(vec![json!("hl7v2"), json!(control), json!(0)]),
        stamp: Some(stamp.to_owned()),
    }
}

/// The body naming `family`.
fn body(family: &str) -> Body {
    Body::new(
        json!({"resourceType": "Patient", "name": [{"family": family}]})
            .to_string()
            .into_bytes(),
    )
}

/// A sealed store, its MLLP source's id and its inbox.
async fn inbox() -> Result<Option<(impl Sized, Uuid, InboxStore)>, Box<dyn Error>> {
    let Some((postgres, store)) = crate::store::sealed_store().await? else {
        return Ok(None);
    };
    let secrets = tempfile::tempdir()?;
    let source = create(&sources(&store, secrets.path())?, &mllp()).await?;
    let inbox = InboxStore::new(store.pool().clone()).with_seal(store.seal().cloned());
    Ok(Some(((postgres, store, secrets), source.id, inbox)))
}

/// The work item of `entity` of `source`.
fn key(source: Uuid, entity: &str) -> WorkKey {
    WorkKey {
        source_id: source,
        tenant: SYSTEM.to_owned(),
        entity: entity.to_owned(),
    }
}

#[tokio::test]
async fn the_body_of_the_applied_generation_is_read_and_cleared_once_applied()
-> Result<(), Box<dyn Error>> {
    let Some((_held, source, inbox)) = inbox().await? else {
        return Ok(());
    };
    let first = event("PAT-1", "MSG-1", "2026-11-25T10:00:00Z");
    assert_eq!(
        Accepted::Stored,
        inbox
            .accept_with_body(source, SYSTEM, &first, &body("Doe"))
            .await?
    );
    let worker = Uuid::new_v4();
    let claimed = inbox.claim(worker, LEASE, 1).await?.remove(0);
    let newer = event("PAT-1", "MSG-2", "2026-11-25T10:05:00Z");
    inbox
        .accept_with_body(source, SYSTEM, &newer, &body("Smith"))
        .await?;
    assert_eq!(
        None,
        inbox.body(&claimed.key, claimed.generation).await?,
        "the claimed generation moved on, so its worker reads no other event's body"
    );
    inbox.give_back(&claimed, worker, Returned::Done).await?;
    let rerun = inbox.claim(worker, LEASE, 1).await?.remove(0);
    let read = inbox
        .body(&rerun.key, rerun.generation)
        .await?
        .ok_or("the newer event's body")?;
    assert!(String::from_utf8(read.as_bytes().to_vec())?.contains("Smith"));
    inbox.give_back(&rerun, worker, Returned::Done).await?;
    let left: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inbound_event WHERE source_id = $1 AND body IS NOT NULL",
    )
    .bind(source)
    .fetch_one(inbox.pool())
    .await?;
    assert_eq!(0, left, "every applied body is cleared");
    Ok(())
}

#[tokio::test]
async fn a_repeated_control_id_changes_nothing_and_an_older_stamp_is_superseded()
-> Result<(), Box<dyn Error>> {
    let Some((_held, source, inbox)) = inbox().await? else {
        return Ok(());
    };
    let current = event("PAT-2", "MSG-1", "2026-11-25T10:05:00Z");
    inbox
        .accept_with_body(source, SYSTEM, &current, &body("Doe"))
        .await?;
    assert_eq!(
        Accepted::Repeated,
        inbox
            .accept_with_body(source, SYSTEM, &current, &body("Doe"))
            .await?
    );
    let older = event("PAT-2", "MSG-0", "2026-11-25T10:00:00Z");
    assert_eq!(
        Accepted::Superseded,
        inbox
            .accept_with_body(source, SYSTEM, &older, &body("Old"))
            .await?
    );
    let claimed = inbox.claim(Uuid::new_v4(), LEASE, 10).await?;
    assert_eq!(1, claimed.len());
    assert_eq!(
        1, claimed[0].generation,
        "neither the repeat nor the older event moved it"
    );
    let kept: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT body FROM inbound_event WHERE source_id = $1 AND superseded")
            .bind(source)
            .fetch_one(inbox.pool())
            .await?;
    assert_eq!(None, kept, "a superseded event keeps no body");
    Ok(())
}

#[tokio::test]
async fn a_discarded_park_clears_its_bodies() -> Result<(), Box<dyn Error>> {
    let Some((_held, source, inbox)) = inbox().await? else {
        return Ok(());
    };
    inbox
        .accept_with_body(
            source,
            SYSTEM,
            &event("PAT-3", "MSG-1", "2026-11-25T10:00:00Z"),
            &body("Doe"),
        )
        .await?;
    let worker = Uuid::new_v4();
    let claimed = inbox.claim(worker, LEASE, 1).await?.remove(0);
    inbox
        .give_back(&claimed, worker, Returned::Park("feed-refused"))
        .await?;
    assert!(
        inbox
            .body(&claimed.key, claimed.generation)
            .await?
            .is_some()
    );
    let decided = Decided {
        decision: Decision::Discard,
        by: String::from("operator-test"),
        reason: String::from("synthetic"),
    };
    assert!(inbox.settle(&key(source, "PAT-3"), &decided).await?);
    assert_eq!(None, inbox.body(&claimed.key, claimed.generation).await?);
    Ok(())
}

#[tokio::test]
async fn a_body_parked_past_its_retention_is_cleared() -> Result<(), Box<dyn Error>> {
    let Some((_held, source, inbox)) = inbox().await? else {
        return Ok(());
    };
    inbox
        .accept_with_body(
            source,
            SYSTEM,
            &event("PAT-4", "MSG-1", "2026-11-25T10:00:00Z"),
            &body("Doe"),
        )
        .await?;
    let worker = Uuid::new_v4();
    let claimed = inbox.claim(worker, LEASE, 1).await?.remove(0);
    inbox
        .give_back(&claimed, worker, Returned::Park("feed-refused"))
        .await?;
    assert_eq!(
        0,
        inbox
            .expire_parked_bodies(Duration::from_secs(3600))
            .await?
    );
    assert_eq!(1, inbox.expire_parked_bodies(Duration::ZERO).await?);
    assert_eq!(None, inbox.body(&claimed.key, claimed.generation).await?);
    Ok(())
}
