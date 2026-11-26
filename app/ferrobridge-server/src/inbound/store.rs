// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbox, the work items and the slot pool in the state store.
//!
//! An event is stored, its entity's work item moved to a new generation and
//! [`INBOUND`] notified in one transaction, before the source is
//! answered. Workers on every replica claim due items with `FOR UPDATE SKIP
//! LOCKED` under a lease through [`crate::store::claim`], make their
//! remote calls outside any transaction, and finish at the generation they
//! saw, so an event that arrived during the work runs the item again. A slot
//! is a leased row taken and given back in two short statements, so no
//! connection is held across a remote call. No specification governs an
//! inbound intake: our own design.

use std::sync::LazyLock;
use std::time::Duration;

use sqlx::postgres::PgPool;
use uuid::Uuid;

use super::event::{Change, Event};
use crate::retry::{Decided, Decision};
use crate::store::claim::{Claim, millis};
use crate::store::listen::INBOUND;
use crate::store::seal::{KeyId, SealKeys};

/// The entity one work item is about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkKey {
    /// The source.
    pub source_id: Uuid,
    /// The tenant as the source document names it.
    pub tenant: String,
    /// The entity as the source names it.
    pub entity: String,
}

/// Whether an event was new.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accepted {
    /// The event is stored and its work item is pending.
    Stored,
    /// The same event was stored before; nothing changed.
    Repeated,
    /// The event is stored as superseded: its stamp is older than the one
    /// its entity applied or waits on, so it moves no work item.
    Superseded,
}

/// The record an event carries, which the store seals before it is written
/// and its worker applies.
#[derive(Clone, PartialEq, Eq)]
pub struct Body(Vec<u8>);

impl Body {
    /// Returns `bytes` as an event's body.
    #[must_use]
    pub const fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Returns the body's bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for Body {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Body(..)")
    }
}

/// Why an event's body could not be stored or read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BodyError {
    /// A statement failed.
    #[error("the inbox statement failed")]
    Database(#[from] sqlx::Error),
    /// The store has no `[state.seal]` keys to seal a body under.
    #[error("the inbox has no [state.seal] keys to seal an event body under")]
    Unsealed,
    /// The body could not be sealed or did not open.
    #[error("an event body could not be sealed or did not open")]
    Seal(#[from] crate::store::seal::SealError),
    /// The stored key id does not read.
    #[error("an event body names a key id that does not read")]
    KeyId,
}

/// A work item a worker holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimed {
    /// The entity.
    pub key: WorkKey,
    /// The generation the worker saw.
    pub generation: i64,
    /// The change the newest event asks for.
    pub change: Change,
    /// The attempts made before this one.
    pub attempts: u32,
}

/// What became of a work item when its worker gave it back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Done, retried or parked as asked.
    Settled,
    /// An event arrived during the work, so the item is pending again.
    Rerun,
    /// The lease ran out and another worker holds the item.
    Lost,
}

/// How a worker gives a work item back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Returned {
    /// The work is done.
    Done,
    /// The attempt failed; run it again after the delay.
    Retry(Duration),
    /// The attempt failed for good; park it with the reason, which names no
    /// value of a record.
    Park(std::borrow::Cow<'static, str>),
    /// Not attempted: no slot was free, or the source is disabled.
    Defer(Duration),
}

/// One parked item, as the operator API lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parked {
    /// The entity.
    pub key: WorkKey,
    /// The fixed reason it parked.
    pub reason: String,
    /// The attempts made.
    pub attempts: u32,
    /// When it parked, RFC 3339 UTC.
    pub since: String,
}

/// What a replay runs again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Replay {
    /// One entity.
    Entity {
        /// The tenant as the source document names it.
        tenant: String,
        /// The entity.
        entity: String,
    },
    /// Every entity an event received in `[from, to)` named.
    Received {
        /// The first instant, inclusive.
        from: jiff::Timestamp,
        /// The last instant, exclusive.
        to: jiff::Timestamp,
    },
}

/// The intake's rows.
#[derive(Debug, Clone)]
pub struct InboxStore {
    /// The state store's pool.
    pool: PgPool,
    /// The keys an event's body is sealed under.
    seal: Option<SealKeys>,
}

/// The claim of due work items: pending items whose time has come, and
/// running items whose lease ran out.
static WORK: LazyLock<String> = LazyLock::new(|| {
    Claim {
        table: "inbound_work",
        keys: &["source_id", "tenant", "entity_key"],
        due: "(state = 'pending' AND next_at <= now()) \
              OR (state = 'running' AND leased_until < now())",
        order: "next_at",
        limit: "$3",
        mark: "state = 'running', holder = $1, \
               leased_until = now() + $2 * interval '1 millisecond', updated_at = now()",
        returning: "claimed.source_id, claimed.tenant, claimed.entity_key, \
                    claimed.generation, claimed.change, claimed.attempts",
    }
    .statement()
});

/// The claim of a free slot of one source and tenant among the first `$3`.
static SLOT: LazyLock<String> = LazyLock::new(|| {
    Claim {
        table: "inbound_slot",
        keys: &["source_id", "tenant", "slot"],
        due: "source_id = $1 AND tenant = $2 AND slot < $3 \
              AND (holder IS NULL OR leased_until < now())",
        order: "slot",
        limit: "1",
        mark: "holder = $4, leased_until = now() + $5 * interval '1 millisecond'",
        returning: "claimed.slot",
    }
    .statement()
});

/// Returns the column text of `change`.
const fn change_text(change: Change) -> &'static str {
    match change {
        Change::Upsert => "upsert",
        Change::Remove => "remove",
    }
}

/// The columns of a claimed row.
type ClaimedRow = (Uuid, String, String, i64, String, i32);

impl InboxStore {
    /// Returns the intake's rows in the store behind `pool`.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool, seal: None }
    }

    /// Returns these rows sealing each event's body under `seal`.
    #[must_use]
    pub fn with_seal(self, seal: Option<SealKeys>) -> Self {
        Self { seal, ..self }
    }

    /// Returns the pool the rows are read and written through.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Returns how many items of `source_id` wait or run.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the statement fails.
    pub async fn pending(&self, source_id: Uuid) -> Result<u64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inbound_work \
             WHERE source_id = $1 AND state IN ('pending', 'running')",
        )
        .bind(source_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.unsigned_abs())
    }

    /// Stores `event` of `source_id` for `tenant`, moves its entity's work item to a new
    /// generation and notifies [`INBOUND`], in one transaction.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when a statement or the commit fails; nothing
    /// is stored then.
    pub async fn accept(
        &self,
        source_id: Uuid,
        tenant: &str,
        event: &Event,
    ) -> Result<Accepted, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let accepted = store_event(&mut tx, source_id, tenant, event, None).await?;
        tx.commit().await?;
        Ok(accepted)
    }

    /// Stores `event` with its record `body` sealed under `[state.seal]`, as
    /// [`Self::accept`] does.
    ///
    /// # Errors
    /// Returns [`BodyError::Unsealed`] without `[state.seal]` keys,
    /// [`BodyError::Seal`] when the body does not seal, and
    /// [`BodyError::Database`] when a statement or the commit fails; nothing
    /// is stored then.
    pub async fn accept_with_body(
        &self,
        source_id: Uuid,
        tenant: &str,
        event: &Event,
        body: &Body,
    ) -> Result<Accepted, BodyError> {
        let keys = self.seal.as_ref().ok_or(BodyError::Unsealed)?;
        let (key_id, sealed) =
            keys.seal(&body_context(source_id, tenant, &event.entity), &body.0)?;
        let mut tx = self.pool.begin().await?;
        let accepted = store_event(
            &mut tx,
            source_id,
            tenant,
            event,
            Some((key_id.as_str(), sealed.as_slice())),
        )
        .await?;
        tx.commit().await?;
        Ok(accepted)
    }

    /// Returns the opened body of the event that moved `key` to
    /// `generation`, `None` when the item moved on or the event has none.
    ///
    /// # Errors
    /// Returns [`BodyError`] when a statement fails, the store has no keys
    /// or the body does not open.
    pub async fn body(&self, key: &WorkKey, generation: i64) -> Result<Option<Body>, BodyError> {
        let row: Option<(Vec<u8>, String)> = sqlx::query_as(
            "SELECT e.body, e.body_key_id FROM inbound_work w \
             JOIN inbound_event e ON e.id = w.event_id \
             WHERE w.source_id = $1 AND w.tenant = $2 AND w.entity_key = $3 \
               AND w.generation = $4 AND e.body IS NOT NULL",
        )
        .bind(key.source_id)
        .bind(&key.tenant)
        .bind(&key.entity)
        .bind(generation)
        .fetch_optional(&self.pool)
        .await?;
        let Some((sealed, key_id)) = row else {
            return Ok(None);
        };
        let keys = self.seal.as_ref().ok_or(BodyError::Unsealed)?;
        let key_id = KeyId::new(&key_id).map_err(|_invalid| BodyError::KeyId)?;
        let opened = keys.open(
            &key_id,
            &body_context(key.source_id, &key.tenant, &key.entity),
            &sealed,
        )?;
        Ok(Some(Body(opened)))
    }

    /// Clears the bodies of the events of items parked longer than `kept`,
    /// returning how many (GDPR Art 5(1)(e)).
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the statement fails.
    pub async fn expire_parked_bodies(&self, kept: Duration) -> Result<u64, sqlx::Error> {
        Ok(sqlx::query(
            "UPDATE inbound_event e SET body = NULL, body_key_id = NULL \
             FROM inbound_work w \
             WHERE e.body IS NOT NULL AND w.state = 'parked' \
               AND w.updated_at < now() - $1 * interval '1 millisecond' \
               AND e.source_id = w.source_id AND e.tenant = w.tenant \
               AND e.entity_key = w.entity_key",
        )
        .bind(millis(kept))
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    /// Begins a transaction on the store.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when no connection is free.
    pub async fn begin(&self) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, sqlx::Error> {
        self.pool.begin().await
    }

    /// Claims at most `limit` due items for `holder` until `lease` passes:
    /// pending items whose time has come, and running items whose lease ran
    /// out.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the statement fails.
    pub async fn claim(
        &self,
        holder: Uuid,
        lease: Duration,
        limit: u32,
    ) -> Result<Vec<Claimed>, sqlx::Error> {
        let rows: Vec<ClaimedRow> = sqlx::query_as(WORK.as_str())
            .bind(holder)
            .bind(millis(lease))
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(
                |(source_id, tenant, entity, generation, change, attempts)| Claimed {
                    key: WorkKey {
                        source_id,
                        tenant,
                        entity,
                    },
                    generation,
                    change: if change == "remove" {
                        Change::Remove
                    } else {
                        Change::Upsert
                    },
                    attempts: attempts.unsigned_abs(),
                },
            )
            .collect())
    }

    /// Gives `item` back as `returned` says, while `holder` still holds it.
    ///
    /// When an event arrived since the item was claimed, a done, retried or
    /// parked item is pending again at once with its attempts reset.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the statement fails.
    pub async fn give_back(
        &self,
        item: &Claimed,
        holder: Uuid,
        returned: Returned,
    ) -> Result<Outcome, sqlx::Error> {
        let (how, delay, reason) = match returned {
            Returned::Done => ("done", Duration::ZERO, None),
            Returned::Retry(delay) => ("retry", delay, None),
            Returned::Park(reason) => ("park", Duration::ZERO, Some(reason)),
            Returned::Defer(delay) => ("defer", delay, None),
        };
        let mut tx = self.pool.begin().await?;
        let same: Option<bool> = sqlx::query_scalar(
            "UPDATE inbound_work SET \
               state = CASE \
                 WHEN $5 = 'defer' OR generation <> $4 THEN 'pending' \
                 WHEN $5 = 'done' THEN 'done' \
                 WHEN $5 = 'park' THEN 'parked' \
                 ELSE 'pending' END, \
               attempts = CASE \
                 WHEN $5 = 'defer' THEN attempts \
                 WHEN $5 = 'done' OR generation <> $4 THEN 0 \
                 ELSE attempts + 1 END, \
               next_at = CASE \
                 WHEN $5 <> 'defer' AND generation <> $4 THEN now() \
                 ELSE now() + $6 * interval '1 millisecond' END, \
               park_reason = CASE WHEN $5 = 'park' AND generation = $4 THEN $7 ELSE NULL END, \
               holder = NULL, leased_until = NULL, updated_at = now() \
             WHERE source_id = $1 AND tenant = $2 AND entity_key = $3 \
               AND holder = $8 AND state = 'running' \
             RETURNING generation = $4",
        )
        .bind(item.key.source_id)
        .bind(&item.key.tenant)
        .bind(&item.key.entity)
        .bind(item.generation)
        .bind(how)
        .bind(millis(delay))
        .bind(reason)
        .bind(holder)
        .fetch_optional(&mut *tx)
        .await?;
        if same == Some(true) && how == "done" {
            applied(&mut tx, &item.key).await?;
        }
        tx.commit().await?;
        Ok(match same {
            None => Outcome::Lost,
            Some(true) => Outcome::Settled,
            Some(false) => Outcome::Rerun,
        })
    }

    /// Takes a free slot of `source_id` and `tenant` among the first
    /// `limit` for `holder` until `lease` passes, `None` while all are held.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when a statement fails.
    pub async fn take_slot(
        &self,
        source_id: Uuid,
        tenant: &str,
        limit: u32,
        holder: Uuid,
        lease: Duration,
    ) -> Result<Option<i32>, sqlx::Error> {
        let bound = i32::try_from(limit).unwrap_or(i32::MAX);
        sqlx::query(
            "INSERT INTO inbound_slot (source_id, tenant, slot) \
             SELECT $1, $2, slot FROM generate_series(0, $3 - 1) AS slot \
             ON CONFLICT DO NOTHING",
        )
        .bind(source_id)
        .bind(tenant)
        .bind(bound)
        .execute(&self.pool)
        .await?;
        sqlx::query_scalar(SLOT.as_str())
            .bind(source_id)
            .bind(tenant)
            .bind(bound)
            .bind(holder)
            .bind(millis(lease))
            .fetch_optional(&self.pool)
            .await
    }

    /// Gives back the slot `slot` of `source_id` and `tenant` that `holder`
    /// holds.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the statement fails.
    pub async fn release_slot(
        &self,
        source_id: Uuid,
        tenant: &str,
        slot: i32,
        holder: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE inbound_slot SET holder = NULL, leased_until = NULL \
             WHERE source_id = $1 AND tenant = $2 AND slot = $3 AND holder = $4",
        )
        .bind(source_id)
        .bind(tenant)
        .bind(slot)
        .bind(holder)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Returns the parked items of `source_id`, oldest first.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the statement fails.
    pub async fn parked(&self, source_id: Uuid) -> Result<Vec<Parked>, sqlx::Error> {
        let rows: Vec<(String, String, String, i32, String)> = sqlx::query_as(
            "SELECT tenant, entity_key, park_reason, attempts, \
               to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') \
             FROM inbound_work WHERE source_id = $1 AND state = 'parked' \
             ORDER BY updated_at, tenant, entity_key",
        )
        .bind(source_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(tenant, entity, reason, attempts, since)| Parked {
                key: WorkKey {
                    source_id,
                    tenant,
                    entity,
                },
                reason,
                attempts: attempts.unsigned_abs(),
                since,
            })
            .collect())
    }

    /// Settles the parked item `key` as `decided` says, recording who
    /// decided and why; `false` when no parked item has the key.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when a statement fails.
    pub async fn settle(&self, key: &WorkKey, decided: &Decided) -> Result<bool, sqlx::Error> {
        let state = match decided.decision {
            Decision::Retry => "pending",
            Decision::Discard => "discarded",
        };
        let mut tx = self.pool.begin().await?;
        let settled = sqlx::query(
            "UPDATE inbound_work SET state = $4, attempts = 0, next_at = now(), \
               settled_by = $5, settled_reason = $6, settled_at = now(), updated_at = now() \
             WHERE source_id = $1 AND tenant = $2 AND entity_key = $3 AND state = 'parked'",
        )
        .bind(key.source_id)
        .bind(&key.tenant)
        .bind(&key.entity)
        .bind(state)
        .bind(&decided.by)
        .bind(&decided.reason)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            > 0;
        if settled && decided.decision == Decision::Retry {
            INBOUND.notify(&mut tx, &key.source_id.to_string()).await?;
        }
        if settled && decided.decision == Decision::Discard {
            clear_bodies(&mut tx, key, None).await?;
        }
        tx.commit().await?;
        Ok(settled)
    }

    /// Runs again the items of `source_id` that `replay` names, each at a new
    /// generation; returns how many.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when a statement fails.
    pub async fn replay(&self, source_id: Uuid, replay: &Replay) -> Result<u64, sqlx::Error> {
        let (tenant, entity, from, to) = match replay {
            Replay::Entity { tenant, entity } => {
                (Some(tenant.as_str()), Some(entity.as_str()), None, None)
            }
            Replay::Received { from, to } => {
                (None, None, Some(from.to_string()), Some(to.to_string()))
            }
        };
        let mut tx = self.pool.begin().await?;
        let replayed = sqlx::query(
            "UPDATE inbound_work SET generation = generation + 1, \
               state = CASE WHEN state = 'running' THEN 'running' ELSE 'pending' END, \
               attempts = CASE WHEN state = 'running' THEN attempts ELSE 0 END, \
               next_at = CASE WHEN state = 'running' THEN next_at ELSE now() END, \
               updated_at = now() \
             WHERE source_id = $1 AND ( \
               ($2::text IS NOT NULL AND tenant = $2 AND entity_key = $3) \
               OR ($2::text IS NULL AND (tenant, entity_key) IN \
                 (SELECT tenant, entity_key FROM inbound_event \
                  WHERE source_id = $1 AND received_at >= $4::timestamptz \
                    AND received_at < $5::timestamptz)))",
        )
        .bind(source_id)
        .bind(tenant)
        .bind(entity)
        .bind(from)
        .bind(to)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if replayed > 0 {
            INBOUND.notify(&mut tx, &source_id.to_string()).await?;
        }
        tx.commit().await?;
        Ok(replayed)
    }
}

/// Why a cursor did not move.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CursorError {
    /// Another replica leads the source's recovery since the cursor was
    /// read.
    #[error("the recovery lease moved on")]
    Fenced(#[source] crate::store::lease::LeaseError),
    /// A statement failed.
    #[error("the cursor statement failed")]
    Database(#[from] sqlx::Error),
}

impl From<crate::store::lease::LeaseError> for CursorError {
    fn from(error: crate::store::lease::LeaseError) -> Self {
        match error {
            crate::store::lease::LeaseError::Database(error) => Self::Database(error),
            other => Self::Fenced(other),
        }
    }
}

impl InboxStore {
    /// Returns the cursor of poll `poll` of `source_id` for `tenant` and when
    /// it last moved, `None` before its first poll.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the statement fails.
    pub async fn cursor(
        &self,
        source_id: Uuid,
        tenant: &str,
        poll: i32,
    ) -> Result<Option<(jiff::Timestamp, jiff::Timestamp)>, sqlx::Error> {
        let row: Option<(i64, i64)> = sqlx::query_as(
            "SELECT (extract(epoch FROM cursor) * 1000000)::bigint, \
               (extract(epoch FROM polled_at) * 1000000)::bigint \
             FROM inbound_cursor WHERE source_id = $1 AND tenant = $2 AND poll = $3",
        )
        .bind(source_id)
        .bind(tenant)
        .bind(poll)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|(cursor, polled_at)| {
            // NOTE: our own design; an instant outside the jiff range is no cursor, so the poll reads from its first window.
            Some((
                jiff::Timestamp::from_microsecond(cursor).ok()?,
                jiff::Timestamp::from_microsecond(polled_at).ok()?,
            ))
        }))
    }

    /// Moves the cursor of poll `poll` of `source_id` for `tenant` to `next`,
    /// refused unless `generation` still holds `lease`.
    ///
    /// # Errors
    /// Returns [`CursorError::Fenced`] when another replica took the lease,
    /// and [`CursorError::Database`] when a statement fails.
    pub async fn move_cursor(
        &self,
        lease: &crate::store::lease::Lease,
        generation: crate::store::lease::Generation,
        source_id: Uuid,
        tenant: &str,
        poll: i32,
        next: jiff::Timestamp,
    ) -> Result<(), CursorError> {
        let mut tx = self.pool.begin().await?;
        lease.fence(&mut tx, generation).await?;
        sqlx::query(
            "INSERT INTO inbound_cursor (source_id, tenant, poll, cursor, polled_at) \
             VALUES ($1, $2, $3, $4::timestamptz, now()) \
             ON CONFLICT (source_id, tenant, poll) DO UPDATE \
             SET cursor = EXCLUDED.cursor, polled_at = now()",
        )
        .bind(source_id)
        .bind(tenant)
        .bind(poll)
        .bind(next.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Marks the work items of `entities` of `source_id` and `tenant` due
    /// with `change`, each at a new generation; returns how many.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when a statement fails.
    pub async fn mark(
        &self,
        source_id: Uuid,
        tenant: &str,
        entities: &[String],
        change: Change,
    ) -> Result<u64, sqlx::Error> {
        if entities.is_empty() {
            return Ok(0);
        }
        let mut tx = self.pool.begin().await?;
        let marked = bump(&mut tx, source_id, tenant, entities, change, None).await?;
        tx.commit().await?;
        Ok(marked)
    }
}

/// Stores `event` of `source_id` for `tenant` in `tx`, with the sealed
/// `body` and the id of the key it is sealed under, and moves its entity's
/// work item to a new generation, notifying [`INBOUND`] when `tx` commits.
///
/// A stored repeat changes nothing. An event that carries its record and
/// whose stamp is older than the stamp its entity applied or waits on is
/// stored as superseded, without its body, and moves no work item; stamps
/// that are not both RFC 3339 instants are not compared. An event without a
/// body always moves its item, since its worker reads the source's current
/// state.
///
/// # Errors
/// Returns [`sqlx::Error`] when a statement fails.
pub async fn store_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    source_id: Uuid,
    tenant: &str,
    event: &Event,
    body: Option<(&str, &[u8])>,
) -> Result<Accepted, sqlx::Error> {
    let superseded = match (event.stamp.as_deref(), body) {
        (Some(stamp), Some(_)) => older(tx, source_id, tenant, &event.entity, stamp).await?,
        _ => false,
    };
    let body = body.filter(|_| !superseded);
    let inserted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO inbound_event (source_id, dedupe_key, tenant, entity_key, change, stamp, \
           body, body_key_id, superseded) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         ON CONFLICT (source_id, dedupe_key) DO NOTHING RETURNING id",
    )
    .bind(source_id)
    .bind(event.dedupe_key.as_bytes().as_slice())
    .bind(tenant)
    .bind(&event.entity)
    .bind(change_text(event.change))
    .bind(event.stamp.as_deref())
    .bind(body.map(|(_, sealed)| sealed))
    .bind(body.map(|(key_id, _)| key_id))
    .bind(superseded)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(id) = inserted else {
        return Ok(Accepted::Repeated);
    };
    if superseded {
        return Ok(Accepted::Superseded);
    }
    bump(
        tx,
        source_id,
        tenant,
        std::slice::from_ref(&event.entity),
        event.change,
        Some(id),
    )
    .await?;
    Ok(Accepted::Stored)
}

/// Returns whether `stamp` is older than the stamp the entity applied or
/// the one its item waits on, locking the item for the rest of `tx`.
async fn older(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    source_id: Uuid,
    tenant: &str,
    entity: &str,
    stamp: &str,
) -> Result<bool, sqlx::Error> {
    // NOTE: our own design; a stamp that is no RFC 3339 instant orders nothing, so it is never older.
    let Ok(stamp) = stamp.parse::<jiff::Timestamp>() else {
        return Ok(false);
    };
    let held: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT w.applied_stamp, e.stamp FROM inbound_work w \
         LEFT JOIN inbound_event e ON e.id = w.event_id \
         WHERE w.source_id = $1 AND w.tenant = $2 AND w.entity_key = $3 \
         FOR UPDATE OF w",
    )
    .bind(source_id)
    .bind(tenant)
    .bind(entity)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((applied, waiting)) = held else {
        return Ok(false);
    };
    Ok([applied, waiting]
        .into_iter()
        .flatten()
        .filter_map(|held| held.parse::<jiff::Timestamp>().ok())
        .any(|held| stamp < held))
}

/// Records that the item `key` applied the event of its generation: its
/// stamp becomes the applied one, and the bodies of that event and every
/// older one of the entity are cleared (GDPR Art 5(1)(e)).
async fn applied(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    key: &WorkKey,
) -> Result<(), sqlx::Error> {
    let event: Option<Option<Uuid>> = sqlx::query_scalar(
        "UPDATE inbound_work w SET applied_stamp = COALESCE( \
           (SELECT e.stamp FROM inbound_event e WHERE e.id = w.event_id), w.applied_stamp) \
         WHERE w.source_id = $1 AND w.tenant = $2 AND w.entity_key = $3 \
         RETURNING w.event_id",
    )
    .bind(key.source_id)
    .bind(&key.tenant)
    .bind(&key.entity)
    .fetch_optional(&mut **tx)
    .await?;
    match event.flatten() {
        Some(event) => clear_bodies(tx, key, Some(event)).await,
        None => Ok(()),
    }
}

/// Clears the bodies of the events of `key`, every one or those up to and
/// including `through`.
async fn clear_bodies(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    key: &WorkKey,
    through: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    // NOTE: our own design; event ids are UUIDv7 (RFC 9562 §5.7), so an id at or below another was stored no later.
    sqlx::query(
        "UPDATE inbound_event SET body = NULL, body_key_id = NULL \
         WHERE source_id = $1 AND tenant = $2 AND entity_key = $3 AND body IS NOT NULL \
           AND ($4::uuid IS NULL OR id <= $4)",
    )
    .bind(key.source_id)
    .bind(&key.tenant)
    .bind(&key.entity)
    .bind(through)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Clears the bodies of items parked longer than `retention` every `tick`,
/// on its own task until `stop` reads `true`.
#[must_use]
pub fn spawn_body_expiry(
    inbox: InboxStore,
    tick: Duration,
    retention: Duration,
    stop: &tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    let mut stop = stop.clone();
    tokio::spawn(async move {
        while !*stop.borrow() {
            if let Err(error) = inbox.expire_parked_bodies(retention).await {
                tracing::warn!(%error, "the parked inbound bodies could not be cleared");
            }
            tokio::select! {
                () = tokio::time::sleep(tick) => {}
                _ = stop.changed() => {}
            }
        }
    })
}

/// Returns the associated data an event body of `entity` is sealed with.
fn body_context(source_id: Uuid, tenant: &str, entity: &str) -> Vec<u8> {
    format!("inbound-event:{source_id}:{tenant}:{entity}").into_bytes()
}

/// Moves the work items of `entities` of `source_id` and `tenant` to a new
/// generation due with `change`, inserting the missing ones, and notifies
/// [`INBOUND`] when `tx` commits; a running item keeps running and reruns
/// when its worker gives it back. An item moved by `event` applies that
/// event's body; a mark or a replay keeps the event it had.
async fn bump(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    source_id: Uuid,
    tenant: &str,
    entities: &[String],
    change: Change,
    event: Option<Uuid>,
) -> Result<u64, sqlx::Error> {
    let bumped = sqlx::query(
            "INSERT INTO inbound_work (source_id, tenant, entity_key, generation, change, state, event_id) \
             SELECT $1, $2, entity, 1, $4, 'pending', $5 FROM unnest($3::text[]) AS entity \
             ON CONFLICT (source_id, tenant, entity_key) DO UPDATE SET \
               generation = inbound_work.generation + 1, \
               change = EXCLUDED.change, \
               event_id = COALESCE(EXCLUDED.event_id, inbound_work.event_id), \
               state = CASE WHEN inbound_work.state = 'running' THEN 'running' ELSE 'pending' END, \
               attempts = CASE WHEN inbound_work.state = 'running' THEN inbound_work.attempts ELSE 0 END, \
               next_at = CASE WHEN inbound_work.state = 'running' THEN inbound_work.next_at ELSE now() END, \
               updated_at = now()",
    )
    .bind(source_id)
    .bind(tenant)
    .bind(entities)
    .bind(change_text(change))
    .bind(event)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    INBOUND.notify(tx, &source_id.to_string()).await?;
    Ok(bumped)
}
