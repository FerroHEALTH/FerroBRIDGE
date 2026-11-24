// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A write on one pool reaches a copy through the `LISTEN` session of
//! another, a rolled-back write reaches none, and a lost session connects
//! again and drops the whole copy.

use std::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ferrobridge_server::store::listen::{Dropping, Listened, SESSION_NAME, Subscriber};
use ferrobridge_testkit::containers;

/// The channel these cases notify.
const CHANNEL: &str = "ferrobridge_test";

/// The longest a case waits for the listener.
const WAIT: Duration = Duration::from_secs(10);

/// A copy that records what it was told to drop.
#[derive(Debug, Default)]
struct Recorded {
    /// The payloads of the notifications, in order.
    payloads: Mutex<Vec<String>>,
    /// The times the whole copy was dropped.
    resets: AtomicU64,
}

impl Recorded {
    fn payloads(&self) -> Vec<String> {
        self.payloads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Subscriber for Recorded {
    fn notified<'a>(&'a self, payload: &'a str) -> Dropping<'a> {
        Box::pin(async move {
            self.payloads
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(payload.to_owned());
        })
    }

    fn reset(&self) -> Dropping<'_> {
        Box::pin(async move {
            self.resets.fetch_add(1, Ordering::AcqRel);
        })
    }
}

#[tokio::test]
async fn a_committed_write_is_heard_on_another_session_and_a_lost_one_reloads()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let listening = super::open(postgres.url()).await?;
    let writing = super::open(postgres.url()).await?;
    let recorded = Arc::new(Recorded::default());
    let listened = Arc::new(Listened::with_subscriber(CHANNEL, recorded.clone()));
    assert!(
        listened.must_read(),
        "a copy is read until the session listens"
    );
    listening.listeners(&[Arc::clone(&listened)]).spawn();
    assert!(listened.listening_within(WAIT).await);
    assert!(
        listened.must_read(),
        "the first connect makes up what was missed"
    );
    assert!(!listened.must_read());

    let seen = listened.notifications();
    let mut rolled_back = writing.pool().begin().await?;
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(CHANNEL)
        .bind("rolled-back")
        .execute(&mut *rolled_back)
        .await?;
    rolled_back.rollback().await?;
    let mut committed = writing.pool().begin().await?;
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(CHANNEL)
        .bind("row-1")
        .execute(&mut *committed)
        .await?;
    committed.commit().await?;
    assert!(listened.notified_after(seen, WAIT).await);
    assert_eq!(vec![String::from("row-1")], recorded.payloads());
    assert!(listened.must_read());
    assert!(!listened.must_read());

    let resets = recorded.resets.load(Ordering::Acquire);
    sqlx::query(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE application_name = $1",
    )
    .bind(SESSION_NAME)
    .execute(writing.pool())
    .await?;
    assert!(listened.lost_after(0, WAIT).await);
    assert!(listened.listening_within(WAIT).await);
    assert!(recorded.resets.load(Ordering::Acquire) >= resets + 2);
    assert!(listened.must_read(), "a reconnect makes up what was missed");
    Ok(())
}
