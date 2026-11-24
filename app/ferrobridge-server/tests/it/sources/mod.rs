// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbound sources across replicas: a change written on one replica is
//! in force on another once its notification arrives, a replica whose
//! listener is down reads the row on every lookup, and a secret gone at
//! runtime fails closed without its value in any error. The cases run only
//! when `FERROBRIDGE_E2E=1` admits the harness.

use std::error::Error;
use std::time::Duration;

use ferrobridge_operator_api::sources::{Kind, Write};
use ferrobridge_server::sources::Sources;
use ferrobridge_server::sources::document::Document;
use ferrobridge_server::store::Store;
use ferrobridge_testkit::containers;
use secrecy::ExposeSecret as _;
use serde_json::json;

/// The value of the synthetic key, which no error may carry.
const KEY_VALUE: &str = "synthetic-subscription-key-91c0";

/// A replica's store on the database `postgres` holds.
async fn replica(postgres: &containers::Postgres) -> Result<Store, Box<dyn Error>> {
    Ok(Store::open(&ferrobridge_server::store::Settings {
        connection: omop_cdm::connection::CdmConnection::new(postgres.url(), None)?,
        max_connections: 4,
        acquire_timeout: Duration::from_secs(30),
        seal: None,
    })
    .await?)
}

/// The sources of `store` resolving secrets in `secrets`.
fn sources(store: &Store, secrets: &std::path::Path) -> Result<Sources, Box<dyn Error>> {
    Ok(Sources::open(
        store,
        Some(&ferrohealth_secret::dir::SecretDirConfig {
            dir: secrets.to_path_buf(),
            poll_seconds: ferrohealth_secret::dir::PollSeconds::default(),
        }),
    )?)
}

/// An MLLP source named `name` routing `application` at `facility` from the
/// loopback network.
fn mllp(name: &str, application: &str, facility: &str) -> Write {
    Write {
        name: name.to_owned(),
        kind: Kind::Mllp,
        enabled: true,
        document: json!({"routes": [{
            "sending_application": application,
            "sending_facility": facility,
            "source_cidrs": ["127.0.0.0/8"]
        }]}),
    }
}

/// Writes `write` as a new source through `sources`.
async fn create(
    sources: &Sources,
    write: &Write,
) -> Result<ferrobridge_server::sources::Source, Box<dyn Error>> {
    let document = sources.validate(write.kind, &write.document)?;
    Ok(sources
        .store()
        .create(write, &document, "operator-test")
        .await?)
}

#[tokio::test]
async fn a_change_on_one_replica_is_in_force_on_another_after_the_notification()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let secrets = tempfile::tempdir()?;
    let (one, two) = (replica(&postgres).await?, replica(&postgres).await?);
    let writer = sources(&one, secrets.path())?;
    let listening = sources(&two, secrets.path())?;
    let listened = listening.listened();
    two.listeners(&[listening.listened()]).spawn();
    assert!(
        listened.listening_within(Duration::from_secs(10)).await,
        "the listener connects"
    );

    let seen = listened.notifications();
    let created = create(&writer, &mllp("device-a", "MONITOR", "WARD-1")).await?;
    assert!(listened.notified_after(seen, Duration::from_secs(10)).await);
    let routed = listening
        .routed("MONITOR", "WARD-1")
        .await?
        .ok_or("routed after the notification")?;
    assert_eq!(created.id, routed.id);

    let seen = listened.notifications();
    let moved = mllp("device-a", "MONITOR", "WARD-2");
    let document = writer.validate(moved.kind, &moved.document)?;
    writer
        .store()
        .update(
            created.id,
            Some(created.version),
            &moved,
            &document,
            "operator-test",
        )
        .await?;
    assert!(listened.notified_after(seen, Duration::from_secs(10)).await);
    assert!(
        listening.routed("MONITOR", "WARD-1").await?.is_none(),
        "the old route is gone"
    );
    assert!(
        listening.routed("MONITOR", "WARD-2").await?.is_some(),
        "the new route is in force"
    );
    Ok(())
}

#[tokio::test]
async fn a_replica_whose_listener_is_down_reads_the_row_on_every_lookup()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let secrets = tempfile::tempdir()?;
    let (one, two) = (replica(&postgres).await?, replica(&postgres).await?);
    let writer = sources(&one, secrets.path())?;
    let unlistened = sources(&two, secrets.path())?;
    assert!(unlistened.named("device-b").await?.is_none());
    let created = create(&writer, &mllp("device-b", "PUMP", "ICU")).await?;
    let read = unlistened
        .named("device-b")
        .await?
        .ok_or("read from its row")?;
    assert_eq!(created.id, read.id);
    assert!(unlistened.routed("PUMP", "ICU").await?.is_some());
    Ok(())
}

#[tokio::test]
async fn a_secret_gone_at_runtime_fails_closed_naming_no_value() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let secrets = tempfile::tempdir()?;
    std::fs::write(secrets.path().join("subscription-key"), KEY_VALUE)?;
    let store = replica(&postgres).await?;
    let held = sources(&store, secrets.path())?;
    let write = Write {
        name: String::from("fhir-sender"),
        kind: Kind::FhirSubscription,
        enabled: true,
        document: json!({"header": {"name": "Authorization", "keys": [{"secret": "subscription-key"}]}}),
    };
    let source = create(&held, &write).await?;
    let Document::FhirSubscription(subscription) = &source.parsed else {
        return Err("a FHIR Subscription document".into());
    };
    let keys = &subscription.header.as_ref().ok_or("a header")?.keys;
    let values = held.keys(&source, keys)?;
    assert_eq!(KEY_VALUE, values.first().ok_or("one key")?.expose_secret());
    assert!(
        !format!("{source:?}").contains(KEY_VALUE),
        "no value in a Debug output"
    );

    std::fs::remove_file(secrets.path().join("subscription-key"))?;
    let gone = sources(&store, secrets.path())?;
    let refused = gone.keys(&source, keys).expect_err("the secret is gone");
    let shown = format!("{refused} {refused:?}");
    assert!(shown.contains("subscription-key"), "{shown}");
    assert!(!shown.contains(KEY_VALUE), "{shown}");
    let response = axum::response::IntoResponse::into_response(refused);
    assert_eq!(http::StatusCode::SERVICE_UNAVAILABLE, response.status());
    Ok(())
}
