// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Two replicas over one state store: a save on one runs on the other once
//! the notification arrives. The cases run only when `FERROBRIDGE_E2E=1`
//! admits the harness. No specification governs the operator surface: our
//! own design.

use std::error::Error;
use std::time::Duration;

use http::StatusCode;

use super::{CONTEXT, Case, MODEL};

/// The longest a case waits for the other replica to swap a revision in.
const WAIT: Duration = Duration::from_secs(30);

#[tokio::test]
async fn a_save_on_one_replica_runs_on_the_other_after_the_notification()
-> Result<(), Box<dyn Error>> {
    let Some((postgres, store)) = crate::store::store().await? else {
        return Ok(());
    };
    let first = Case::over(&store, None).await?;
    let second_store = crate::store::open(postgres.url()).await?;
    let second = Case::over(&second_store, Some(postgres)).await?;
    let listened = ferrobridge_server::operator::watch::start(&second.operator)
        .ok_or("the second replica saves revisions")?;
    second_store.listeners(&[listened]).spawn();
    for (name, text) in [
        ("ferrobridge_console.yml", MODEL),
        ("ferrobridge_console.context.yml", CONTEXT),
    ] {
        assert_eq!(
            StatusCode::OK,
            first.create(name, text).await?.status(),
            "{name}"
        );
    }
    let observation = String::from("Observation");
    assert!(first.declared_types().await?.contains(&observation));
    let deadline = tokio::time::Instant::now() + WAIT;
    while !second.declared_types().await?.contains(&observation) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the second replica runs the saved revision within {WAIT:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        Some("2"),
        second.operator.snapshot().console().revision(),
        "the second replica runs the revision the first recorded"
    );
    Ok(())
}
