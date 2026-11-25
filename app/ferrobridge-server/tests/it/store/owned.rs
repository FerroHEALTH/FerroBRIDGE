// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resources the facade keeps itself, in the state store: versioned
//! writes, a refusal that writes nothing, tombstones, and one winner between
//! two replicas racing on one version.

use std::error::Error;

use ferrobridge_server::facade::owned::{Change, Held, OwnedStore};
use fhir_types::codec::{Object, Value};

/// A `Basic` the client sends.
fn basic() -> Result<Object, serde_json::Error> {
    serde_json::from_str(r#"{"resourceType":"Basic","code":{"text":"x"}}"#)
}

/// The id a created resource carries.
fn id_of(created: &Object) -> Result<String, Box<dyn Error>> {
    match created.get("id") {
        Some(Value::String(id)) => Ok(id.clone()),
        _ => Err("an id".into()),
    }
}

#[tokio::test]
async fn a_created_resource_carries_its_id_and_first_version() -> Result<(), Box<dyn Error>> {
    let Some((_postgres, state)) = super::sealed_store().await? else {
        return Ok(());
    };
    let store = OwnedStore::open(&state);
    let created = store.create("Basic", basic()?).await?;
    let id = id_of(&created)?;
    let Some(Held::Live { version, resource }) = store.get("Basic", &id).await? else {
        return Err("live".into());
    };
    assert_eq!(1, version);
    assert_eq!(created, resource);
    let serialized = serde_json::to_value(&resource)?;
    assert_eq!(Some("1"), serialized["meta"]["versionId"].as_str());
    Ok(())
}

#[tokio::test]
async fn a_change_moves_the_version_and_a_refusal_writes_nothing() -> Result<(), Box<dyn Error>> {
    let Some((_postgres, state)) = super::sealed_store().await? else {
        return Ok(());
    };
    let store = OwnedStore::open(&state);
    let id = id_of(&store.create("Basic", basic()?).await?)?;
    let refused = store
        .change("Basic", &id, |_held| Err::<Change, &str>("no"))
        .await?;
    assert_eq!(Err("no"), refused);
    let put = basic()?;
    let updated = store
        .change("Basic", &id, move |_held| Ok::<_, ()>(Change::Put(put)))
        .await?;
    assert_eq!(Ok(2), updated.map(|held| held.version()));
    let deleted = store
        .change("Basic", &id, |_held| Ok::<_, ()>(Change::Delete))
        .await?;
    assert_eq!(Ok(Held::Deleted { version: 3 }), deleted);
    assert_eq!(
        Some(Held::Deleted { version: 3 }),
        store.get("Basic", &id).await?
    );
    assert_eq!(Vec::<Object>::new(), store.scan("Basic").await?);
    assert_eq!(None, store.get("Basic", "never-written").await?);
    Ok(())
}

#[tokio::test]
async fn a_scan_reads_one_type_only() -> Result<(), Box<dyn Error>> {
    let Some((_postgres, state)) = super::sealed_store().await? else {
        return Ok(());
    };
    let store = OwnedStore::open(&state);
    store.create("Basic", basic()?).await?;
    store.create("BasicX", basic()?).await?;
    store.create("Task", basic()?).await?;
    assert_eq!(1, store.scan("Basic").await?.len());
    Ok(())
}

#[tokio::test]
async fn two_replicas_writing_one_version_cannot_both_win() -> Result<(), Box<dyn Error>> {
    let Some((postgres, state)) = super::sealed_store().await? else {
        return Ok(());
    };
    let first = OwnedStore::open(&state);
    let second = OwnedStore::open(&super::open_sealed(postgres.url(), super::KEY_B).await?);
    let id = id_of(&first.create("Basic", basic()?).await?)?;
    let at_one = |held: Option<&Held>, put: Object| match held {
        Some(held) if held.version() == 1 => Ok(Change::Put(put)),
        _ => Err("the version moved"),
    };
    let (left, right) = (basic()?, basic()?);
    let (one, two) = tokio::join!(
        first.change("Basic", &id, move |held| at_one(held, left)),
        second.change("Basic", &id, move |held| at_one(held, right)),
    );
    let outcomes = [one?, two?];
    assert_eq!(1, outcomes.iter().filter(|outcome| outcome.is_ok()).count());
    assert_eq!(
        Some(2),
        first.get("Basic", &id).await?.map(|held| held.version())
    );
    Ok(())
}
