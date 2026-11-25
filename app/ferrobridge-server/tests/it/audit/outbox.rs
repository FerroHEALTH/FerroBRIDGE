// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The audit outbox across replicas: two forwarders over one state store send
//! each record once, and a record a replica stored or claimed before it
//! stopped is sent by another. The cases run only when `FERROBRIDGE_E2E=1`
//! admits the harness.
//!
//! Every record is synthetic: a minimal `AuditEvent` with an invented id.

use std::collections::BTreeMap;
use std::error::Error;
use std::time::Duration;

use ferrobridge_server::audit::outbox::StateOutbox;
use ferrobridge_server::store::Store;
use ihe_atna::atna::forwarder::Outbox;
use secrecy::SecretSlice;

/// The CDR the lanes of these cases name as the facade's origin.
const CDR: &str = "http://cdr.invalid/";

/// The synthetic record numbered `n`.
fn record(n: usize) -> SecretSlice<u8> {
    SecretSlice::from(
        serde_json::to_vec(&serde_json::json!({
            "resourceType": "AuditEvent",
            "id": format!("synthetic-{n}")
        }))
        .unwrap_or_default(),
    )
}

/// A second replica: its own pool on the database `store` reaches.
async fn replica(
    postgres: &ferrobridge_testkit::containers::Postgres,
) -> Result<Store, Box<dyn Error>> {
    Ok(Store::open(&ferrobridge_server::store::Settings {
        connection: omop_cdm::connection::CdmConnection::new(postgres.url(), None)?,
        max_connections: 4,
        acquire_timeout: Duration::from_secs(30),
        seal: None,
    })
    .await?)
}

/// Returns how many times the repository received each record id, once it
/// holds `count` records.
async fn received(
    arr: &wiremock::MockServer,
    count: usize,
) -> Result<BTreeMap<String, usize>, Box<dyn Error>> {
    let mut seen = BTreeMap::new();
    for record in super::records(arr, count).await? {
        let id = record["id"].as_str().unwrap_or_default().to_owned();
        *seen.entry(id).or_insert(0) += 1;
    }
    Ok(seen)
}

/// Returns how many records the outbox of `store` still holds.
async fn waiting(store: &Store) -> Result<i64, Box<dyn Error>> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM audit_outbox")
        .fetch_one(store.pool())
        .await?)
}

#[tokio::test]
async fn two_replicas_forwarding_together_send_each_record_once() -> Result<(), Box<dyn Error>> {
    let Some((postgres, first)) = super::state().await? else {
        return Ok(());
    };
    let second = replica(&postgres).await?;
    let outbox = StateOutbox::new(first.pool().clone());
    for n in 0..40 {
        outbox.store(&record(n)).await?;
    }
    let arr = super::repository().await;
    let one = super::access_log(&arr, &first, &CDR.parse()?, None).await?;
    let two = super::access_log(&arr, &second, &CDR.parse()?, None).await?;
    let seen = received(&arr, 40).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let again = received(&arr, 40).await?;
    assert_eq!(40, again.len(), "every record arrived");
    assert!(
        again.values().all(|times| *times == 1),
        "no record was sent twice: {again:?}"
    );
    assert_eq!(seen, again);
    assert_eq!(
        0,
        waiting(&first).await?,
        "every delivered record left the outbox"
    );
    one.stop();
    two.stop();
    Ok(())
}

#[tokio::test]
async fn a_record_a_stopped_replica_stored_is_sent_by_another() -> Result<(), Box<dyn Error>> {
    let Some((postgres, survivor)) = super::state().await? else {
        return Ok(());
    };
    let killed = replica(&postgres).await?;
    StateOutbox::new(killed.pool().clone())
        .store(&record(1))
        .await?;
    killed.pool().close().await;
    let arr = super::repository().await;
    let log = super::access_log(&arr, &survivor, &CDR.parse()?, None).await?;
    let seen = received(&arr, 1).await?;
    assert_eq!(Some(&1), seen.get("synthetic-1"));
    log.stop();
    Ok(())
}

#[tokio::test]
async fn a_record_a_stopped_replica_claimed_is_sent_by_another_once_its_lease_runs_out()
-> Result<(), Box<dyn Error>> {
    let Some((postgres, survivor)) = super::state().await? else {
        return Ok(());
    };
    let killed = replica(&postgres).await?;
    let claiming = StateOutbox::with_lease(killed.pool().clone(), Duration::from_millis(300));
    claiming.store(&record(2)).await?;
    let claimed = claiming.oldest().await?.ok_or("the record is claimed")?;
    assert!(
        claiming.oldest().await?.is_none(),
        "a claimed record is out of reach of another claim"
    );
    drop(claimed);
    killed.pool().close().await;
    let arr = super::repository().await;
    let log = super::access_log(&arr, &survivor, &CDR.parse()?, None).await?;
    let seen = received(&arr, 1).await?;
    assert_eq!(Some(&1), seen.get("synthetic-2"));
    assert_eq!(0, waiting(&survivor).await?);
    log.stop();
    Ok(())
}
