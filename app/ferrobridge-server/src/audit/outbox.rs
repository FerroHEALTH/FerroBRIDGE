// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The audit outbox in the state store: the recorder that stores each record
//! in `audit_outbox` and the outbox the `ihe-atna` forwarder delivers from.
//!
//! ITI TF-2 §3.20.4.1.1 has a sender store a record it cannot deliver and
//! send it when it can, so the recorder returns once the row is committed and
//! the forwarder sends it over the ATX: FHIR Feed. Every replica stores into
//! one table and runs a forwarder over it: a forwarder claims the oldest
//! unclaimed row for [`LEASE`] with `FOR UPDATE SKIP LOCKED`
//! (<https://www.postgresql.org/docs/current/sql-select.html#SQL-FOR-UPDATE-SHARE>),
//! deletes it once the repository took it and moves it to `audit_quarantine`
//! once the repository refused it, so two replicas never send one record and
//! a record a stopped replica claimed is sent by another once the lease runs
//! out. Where the records wait is our own design.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use ihe_atna::atna::forwarder::{Outbox, OutboxError};
use ihe_atna::atna::spool::{Depth, Stored};
use ihe_atna::balp::AuditError;
use ihe_atna::balp::AuditRecorder;
use ihe_atna::balp::Exchange;
use ihe_atna::balp::Observer;
use ihe_atna::balp::Reserved;
use secrecy::{ExposeSecret as _, SecretSlice};
use sqlx::postgres::PgPool;
use tokio::sync::Notify;

/// How long a forwarder holds a claimed record before another may send it.
// NOTE: no specification governs this: our own design; one delivery round of the FHIR Feed
// ends well within it, so a live forwarder settles its claim first.
pub const LEASE: Duration = Duration::from_secs(120);

/// How long a forwarder that found nothing waits before it reads again, so a
/// record another replica stored is sent without its wake.
const IDLE: Duration = Duration::from_secs(1);

/// The forwarder's error for a store failure.
fn outbox_error(error: impl std::error::Error + Send + Sync + 'static) -> OutboxError {
    OutboxError(Box::new(error))
}

/// The recorder's error for a store failure.
fn audit_error(error: impl std::error::Error + Send + Sync + 'static) -> AuditError {
    AuditError(Box::new(error))
}

/// The audit outbox of the state store behind one pool.
#[derive(Debug)]
pub struct StateOutbox {
    /// The state store's pool.
    pool: PgPool,
    /// How long a claim holds a record.
    lease: Duration,
    /// Woken once this replica stores a record.
    stored: Arc<Notify>,
    /// The records awaiting delivery at the last read.
    waiting: AtomicUsize,
    /// The records in quarantine at the last read.
    quarantined: AtomicUsize,
}

impl StateOutbox {
    /// Returns the outbox the state store behind `pool` keeps, each claim
    /// holding a record for [`LEASE`].
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self::with_lease(pool, LEASE)
    }

    /// Returns the outbox the state store behind `pool` keeps, each claim
    /// holding a record for `lease`.
    #[must_use]
    pub fn with_lease(pool: PgPool, lease: Duration) -> Self {
        Self {
            pool,
            lease,
            stored: Arc::new(Notify::new()),
            waiting: AtomicUsize::new(0),
            quarantined: AtomicUsize::new(0),
        }
    }

    /// Stores `message` and wakes this replica's forwarder.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the row does not commit.
    pub async fn store(&self, message: &SecretSlice<u8>) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO audit_outbox (message) VALUES ($1)")
            .bind(message.expose_secret())
            .execute(&self.pool)
            .await?;
        self.stored.notify_one();
        Ok(())
    }

    /// Returns whether the store answers, so a record can be taken.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when it does not.
    async fn ready(&self) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Outbox for StateOutbox {
    async fn oldest(&self) -> Result<Option<Stored>, OutboxError> {
        let lease_ms = i64::try_from(self.lease.as_millis()).unwrap_or(i64::MAX);
        let claimed: Option<(i64, Vec<u8>)> = sqlx::query_as(
            "UPDATE audit_outbox SET claimed_until = clock_timestamp() + $1 * interval '1 millisecond' \
             WHERE seq = (SELECT seq FROM audit_outbox \
                 WHERE claimed_until IS NULL OR claimed_until < clock_timestamp() \
                 ORDER BY seq LIMIT 1 FOR UPDATE SKIP LOCKED) \
             RETURNING seq, message",
        )
        .bind(lease_ms)
        .fetch_optional(&self.pool)
        .await
        .map_err(outbox_error)?;
        let (waiting, quarantined): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM audit_outbox), (SELECT count(*) FROM audit_quarantine)",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(outbox_error)?;
        self.waiting.store(
            usize::try_from(waiting).unwrap_or(usize::MAX),
            Ordering::Relaxed,
        );
        self.quarantined.store(
            usize::try_from(quarantined).unwrap_or(usize::MAX),
            Ordering::Relaxed,
        );
        let Some((sequence, message)) = claimed else {
            return Ok(None);
        };
        Ok(Some(Stored {
            sequence: u64::try_from(sequence).map_err(outbox_error)?,
            message: SecretSlice::from(message),
        }))
    }

    async fn stored(&self) {
        tokio::select! {
            () = self.stored.notified() => {}
            () = tokio::time::sleep(IDLE) => {}
        }
    }

    async fn remove(&self, sequence: u64) -> Result<(), OutboxError> {
        let sequence = i64::try_from(sequence).map_err(outbox_error)?;
        sqlx::query("DELETE FROM audit_outbox WHERE seq = $1")
            .bind(sequence)
            .execute(&self.pool)
            .await
            .map_err(outbox_error)?;
        Ok(())
    }

    async fn quarantine(&self, sequence: u64) -> Result<(), OutboxError> {
        let sequence = i64::try_from(sequence).map_err(outbox_error)?;
        tracing::error!(
            seq = sequence,
            "the Audit Record Repository refused an access record; it is kept in quarantine for an operator"
        );
        sqlx::query(
            "WITH moved AS (DELETE FROM audit_outbox WHERE seq = $1 RETURNING seq, message, recorded_at) \
             INSERT INTO audit_quarantine (seq, message, recorded_at) SELECT seq, message, recorded_at FROM moved \
             ON CONFLICT (seq) DO NOTHING",
        )
        .bind(sequence)
        .execute(&self.pool)
        .await
        .map_err(outbox_error)?;
        Ok(())
    }

    fn depth(&self) -> Depth {
        let quarantined = self.quarantined.load(Ordering::Relaxed);
        Depth {
            messages: self
                .waiting
                .load(Ordering::Relaxed)
                .saturating_add(quarantined),
            bytes: 0,
            quarantined,
        }
    }

    fn is_durable(&self) -> bool {
        true
    }
}

/// The recorder every access record goes through: it writes the BALP
/// `AuditEvent` and stores it in the outbox, naming the bridge as the
/// recording system.
#[derive(Debug)]
pub(super) struct OutboxRecorder {
    /// The outbox the records are stored in.
    outbox: Arc<StateOutbox>,
    /// The recording system.
    observer: Arc<Observer>,
}

impl OutboxRecorder {
    /// Returns the recorder over `outbox`.
    pub(super) fn new(outbox: Arc<StateOutbox>, observer: Observer) -> Self {
        Self {
            outbox,
            observer: Arc::new(observer),
        }
    }
}

/// Returns the `AuditEvent` `exchange` is written as.
fn written(exchange: &Exchange, observer: &Observer) -> Result<SecretSlice<u8>, AuditError> {
    exchange
        .audit_event(observer)
        .map(ihe_atna::balp::AuditRecord::into_bytes)
        .map_err(audit_error)
}

#[async_trait::async_trait]
impl AuditRecorder for OutboxRecorder {
    async fn record(&self, exchange: Exchange) -> Result<(), AuditError> {
        let record = written(&exchange, &self.observer)?;
        self.outbox.store(&record).await.map_err(audit_error)
    }

    async fn reserve(&self) -> Result<Option<Box<dyn Reserved>>, AuditError> {
        self.outbox.ready().await.map_err(audit_error)?;
        Ok(Some(Box::new(Place {
            outbox: Arc::clone(&self.outbox),
            observer: Arc::clone(&self.observer),
        })))
    }
}

/// One record's place: the outbox answered before the exchange was made.
#[derive(Debug)]
struct Place {
    /// The outbox.
    outbox: Arc<StateOutbox>,
    /// The recording system.
    observer: Arc<Observer>,
}

#[async_trait::async_trait]
impl Reserved for Place {
    async fn record(self: Box<Self>, exchange: Exchange) -> Result<(), AuditError> {
        let record = written(&exchange, &self.observer)?;
        self.outbox.store(&record).await.map_err(audit_error)
    }
}
