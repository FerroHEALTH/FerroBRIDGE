// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity map in the state store: one winner between two replicas, no
//! identifier value in clear, a store of another derivation or lookup key
//! refused at start, one entry recorded whole, and the copy that serves only
//! found rows while its session listens.

use std::error::Error;
use std::sync::Arc;

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::identity::record::{
    CompositionBinding, Identifier, IdentifierQuery, SourceVersion, SystemMatch,
};
use ferrobridge_server::identity::state::StateStore;
use ferrobridge_server::identity::store::{EntryWrite, Store as _, StoreError};
use ferrobridge_server::identity::{ExternalResourceId, FhirResourceId, PersonId};
use ferrobridge_server::store::listen::SESSION_NAME;
use ferrobridge_testkit::containers;

/// A person identifier shaped like a BSN, synthetic.
const BSN: &str = "999999990";

/// The binding of `uid`.
fn binding(uid: &str) -> CompositionBinding {
    CompositionBinding {
        ehr_id: String::from("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11"),
        versioned_object_uid: String::from(uid),
        template_id: String::from("ferrobridge.diagnose.v1"),
        resource_type: String::from("Condition"),
        entry_path: String::from("/content[1]"),
        split: 0,
        context: String::from("ferrobridge_diagnosis.context"),
        derivation: Some(ferrobridge_server::identity::derive::DERIVATION),
        created: None,
    }
}

#[tokio::test]
async fn two_replicas_recording_one_binding_agree_on_one_winner() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let first = StateStore::open(&super::open_sealed(postgres.url(), super::KEY_B).await?).await?;
    let second = StateStore::open(&super::open_sealed(postgres.url(), super::KEY_B).await?).await?;
    let id = FhirResourceId::new("abc")?;
    let one = binding("uid-1");
    let two = binding("uid-2");
    let (left, right) = tokio::join!(
        first.record_binding("Condition", &id, &one),
        second.record_binding("Condition", &id, &two),
    );
    let (left, right) = (left?, right?);
    assert_eq!(left, right, "both replicas answer the binding that stands");
    assert!(left == one || left == two);
    assert_eq!(
        Some(left.clone()),
        first.binding_of("Condition", &id).await?
    );
    assert_eq!(Some(left), second.binding_of("Condition", &id).await?);
    Ok(())
}

#[tokio::test]
async fn no_identifier_value_is_stored_in_clear() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = super::open_sealed(postgres.url(), super::KEY_B).await?;
    let map = StateStore::open(&store).await?;
    let person = PersonId::new("http://fhir.nl/fhir/NamingSystem/bsn", BSN)?;
    let ehr = EhrId::new("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11")?;
    let id = FhirResourceId::new("patient-1")?;
    assert_eq!(ehr, map.record_ehr(&person, &ehr).await?);
    assert_eq!(Some(ehr.clone()), map.ehr_of(&person).await?);
    assert_eq!(Some(person.clone()), map.person_of(&ehr).await?);
    let identifier = Identifier::new(Some("http://fhir.nl/fhir/NamingSystem/bsn"), BSN);
    map.record_identifier("Patient", &identifier, &id).await?;
    for query in [
        IdentifierQuery::new(SystemMatch::Any, BSN),
        IdentifierQuery::new(
            SystemMatch::Is(String::from("http://fhir.nl/fhir/NamingSystem/bsn")),
            BSN,
        ),
    ] {
        assert_eq!(vec![id.clone()], map.identified("Patient", &query).await?);
    }
    let absent = map
        .identified("Patient", &IdentifierQuery::new(SystemMatch::Absent, BSN))
        .await?;
    assert_eq!(Vec::<FhirResourceId>::new(), absent);
    let clear: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ( \
           SELECT id_hmac AS bytes, ehr_id || namespace AS text FROM person_ehr \
           UNION ALL SELECT person_sealed, ehr_id || key_id FROM ehr_subject \
           UNION ALL SELECT value_hmac, resource_type || system || internal_id FROM resource_identifier \
         ) AS stored \
         WHERE position(convert_to($1, 'UTF8') IN bytes) > 0 OR text LIKE '%' || $1 || '%'",
    )
    .bind(BSN)
    .fetch_one(store.pool())
    .await?;
    assert_eq!(0, clear);
    let rows: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM person_ehr) + (SELECT count(*) FROM ehr_subject) \
         + (SELECT count(*) FROM resource_identifier)",
    )
    .fetch_one(store.pool())
    .await?;
    assert_eq!(3, rows, "the scan read the rows it vouches for");
    Ok(())
}

#[tokio::test]
async fn a_store_of_another_derivation_or_lookup_key_refuses_start() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = super::open_sealed(postgres.url(), super::KEY_B).await?;
    StateStore::open(&store).await?;
    let other_key = super::open_sealed(postgres.url(), super::KEY_A).await?;
    assert!(matches!(
        StateStore::open(&other_key).await,
        Err(StoreError::LookupKey)
    ));
    sqlx::query("UPDATE identity_meta SET derivation = derivation - 1")
        .execute(store.pool())
        .await?;
    assert!(matches!(
        StateStore::open(&store).await,
        Err(StoreError::Derivation { .. })
    ));
    let unsealed = super::open(postgres.url()).await?;
    assert!(matches!(
        StateStore::open(&unsealed).await,
        Err(StoreError::Unsealed)
    ));
    Ok(())
}

/// The longest a case waits for the listener.
const WAIT: std::time::Duration = std::time::Duration::from_secs(10);

#[tokio::test]
async fn one_entry_is_recorded_whole_and_its_first_record_stands() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let map = StateStore::open(&super::open_sealed(postgres.url(), super::KEY_B).await?).await?;
    let key = ExternalResourceId::new("entry-key-1")?;
    let derived = FhirResourceId::new("entry-1")?;
    let identifier = Identifier::new(Some("urn:oid:2.999.1.2"), "synthetic-condition-1");
    let source = SourceVersion::new(
        "Condition",
        ExternalResourceId::new("sender-1")?,
        Some(String::from("1")),
    );
    let first = binding("uid-1");
    let (id, stood) = map
        .record_entry(&EntryWrite {
            resource_type: "Condition",
            key: &key,
            derived: &derived,
            binding: &first,
            identifiers: std::slice::from_ref(&identifier),
            source: Some(&source),
        })
        .await?;
    assert_eq!((derived.clone(), first.clone()), (id.clone(), stood));
    assert_eq!(Some(id.clone()), map.internal_of("Condition", &key).await?);
    assert_eq!(Some(first.clone()), map.binding_of("Condition", &id).await?);
    let query = IdentifierQuery::new(
        SystemMatch::Is(String::from("urn:oid:2.999.1.2")),
        "synthetic-condition-1",
    );
    assert_eq!(vec![id.clone()], map.identified("Condition", &query).await?);
    let consumed = map
        .consumed(&source)
        .await?
        .ok_or("the source is consumed")?;
    assert_eq!(id.as_str(), consumed.internal_id);
    assert_eq!(first.versioned_object_uid, consumed.versioned_object_uid);

    let again = map
        .record_entry(&EntryWrite {
            resource_type: "Condition",
            key: &key,
            derived: &FhirResourceId::new("entry-2")?,
            binding: &binding("uid-2"),
            identifiers: &[],
            source: None,
        })
        .await?;
    assert_eq!((id, first), again, "the first record stands");
    Ok(())
}

#[tokio::test]
async fn the_copy_serves_found_rows_while_listening_and_drops_them_on_a_loss()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = super::open_sealed(postgres.url(), super::KEY_B).await?;
    let map = StateStore::open(&store).await?.with_copy(8);
    let other = StateStore::open(&store).await?;
    let listened = map.listened().ok_or("a copy of eight entries")?;
    store.listeners(&[Arc::clone(&listened)]).spawn();
    assert!(listened.listening_within(WAIT).await);

    let missing = FhirResourceId::new("written-later")?;
    assert_eq!(None, map.binding_of("Condition", &missing).await?);
    other
        .record_binding("Condition", &missing, &binding("uid-later"))
        .await?;
    assert_eq!(
        Some(binding("uid-later")),
        map.binding_of("Condition", &missing).await?,
        "a miss is never copied"
    );

    let id = FhirResourceId::new("copied")?;
    map.record_binding("Condition", &id, &binding("uid-1"))
        .await?;
    sqlx::query("DELETE FROM composition_binding WHERE internal_id = $1")
        .bind(id.as_str())
        .execute(store.pool())
        .await?;
    assert_eq!(
        Some(binding("uid-1")),
        map.binding_of("Condition", &id).await?,
        "a found row is served from the copy"
    );

    sqlx::query(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE application_name = $1",
    )
    .bind(SESSION_NAME)
    .execute(store.pool())
    .await?;
    assert!(listened.lost_after(0, WAIT).await);
    assert!(listened.listening_within(WAIT).await);
    assert_eq!(
        None,
        map.binding_of("Condition", &id).await?,
        "a lost session drops the copy"
    );
    Ok(())
}
