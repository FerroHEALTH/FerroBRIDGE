// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbox, the work items and the slot pool in the state store.
//!
//! An event is stored, its entity's work item moved to a new generation and
//! `ferrobridge_inbound` notified in one transaction, before the source is
//! answered. Workers on every replica claim due items with `FOR UPDATE SKIP
//! LOCKED` under a lease (PostgreSQL 19, "The Locking Clause"), make their
//! remote calls outside any transaction, and finish at the generation they
//! saw, so an event that arrived during the work runs the item again. A slot
//! is a leased row taken and given back in two short statements, so no
//! connection is held across a remote call. No specification governs an
//! inbound intake: our own design.

use std::time::Duration;

use sqlx::postgres::PgPool;
use uuid::Uuid;

use super::event::{Change, Event};

/// The channel an accepted event notifies, with its source id as payload.
pub const CHANNEL: &str = "ferrobridge_inbound";

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Returned {
    /// The work is done.
    Done,
    /// The attempt failed; run it again after the delay.
    Retry(Duration),
    /// The attempt failed for good; park it with the fixed reason, which
    /// names no value.
    Park(&'static str),
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

/// How an operator settles a parked item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settlement {
    /// Run it again.
    Retry,
    /// Leave it unapplied for good.
    Discard,
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
}

/// Returns `duration` in whole milliseconds, saturating.
fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

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
        Self { pool }
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
    /// generation and notifies [`CHANNEL`], in one transaction.
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
        let change = change_text(event.change);
        let mut tx = self.pool.begin().await?;
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO inbound_event (source_id, dedupe_key, tenant, entity_key, change, stamp) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (source_id, dedupe_key) DO NOTHING RETURNING id",
        )
        .bind(source_id)
        .bind(event.dedupe_key.as_bytes().as_slice())
        .bind(tenant)
        .bind(&event.entity)
        .bind(change)
        .bind(event.stamp.as_deref())
        .fetch_optional(&mut *tx)
        .await?;
        if inserted.is_none() {
            tx.commit().await?;
            return Ok(Accepted::Repeated);
        }
        sqlx::query(
            "INSERT INTO inbound_work (source_id, tenant, entity_key, generation, change, state) \
             VALUES ($1, $2, $3, 1, $4, 'pending') \
             ON CONFLICT (source_id, tenant, entity_key) DO UPDATE SET \
               generation = inbound_work.generation + 1, \
               change = EXCLUDED.change, \
               state = CASE WHEN inbound_work.state = 'running' THEN 'running' ELSE 'pending' END, \
               attempts = CASE WHEN inbound_work.state = 'running' THEN inbound_work.attempts ELSE 0 END, \
               next_at = CASE WHEN inbound_work.state = 'running' THEN inbound_work.next_at ELSE now() END, \
               updated_at = now()",
        )
        .bind(source_id)
        .bind(tenant)
        .bind(&event.entity)
        .bind(change)
        .execute(&mut *tx)
        .await?;
        notify(&mut tx, source_id).await?;
        tx.commit().await?;
        Ok(Accepted::Stored)
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
        let rows: Vec<ClaimedRow> = sqlx::query_as(
            "WITH due AS MATERIALIZED ( \
               SELECT source_id, tenant, entity_key FROM inbound_work \
               WHERE (state = 'pending' AND next_at <= now()) \
                  OR (state = 'running' AND leased_until < now()) \
               ORDER BY next_at LIMIT $3 FOR UPDATE SKIP LOCKED) \
             UPDATE inbound_work w SET state = 'running', holder = $1, \
               leased_until = now() + $2 * interval '1 millisecond', updated_at = now() \
             FROM due \
             WHERE w.source_id = due.source_id AND w.tenant = due.tenant \
               AND w.entity_key = due.entity_key \
             RETURNING w.source_id, w.tenant, w.entity_key, w.generation, w.change, w.attempts",
        )
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
        .fetch_optional(&self.pool)
        .await?;
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
        sqlx::query_scalar(
            "WITH free AS MATERIALIZED ( \
               SELECT slot FROM inbound_slot \
               WHERE source_id = $1 AND tenant = $2 AND slot < $3 \
                 AND (holder IS NULL OR leased_until < now()) \
               ORDER BY slot LIMIT 1 FOR UPDATE SKIP LOCKED) \
             UPDATE inbound_slot s SET holder = $4, \
               leased_until = now() + $5 * interval '1 millisecond' \
             FROM free \
             WHERE s.source_id = $1 AND s.tenant = $2 AND s.slot = free.slot \
             RETURNING s.slot",
        )
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

    /// Settles the parked item `key` as `settlement` says, recording `by`
    /// and `reason`; `false` when no parked item has the key.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when a statement fails.
    pub async fn settle(
        &self,
        key: &WorkKey,
        settlement: Settlement,
        by: &str,
        reason: &str,
    ) -> Result<bool, sqlx::Error> {
        let state = match settlement {
            Settlement::Retry => "pending",
            Settlement::Discard => "discarded",
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
        .bind(by)
        .bind(reason)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            > 0;
        if settled && settlement == Settlement::Retry {
            notify(&mut tx, key.source_id).await?;
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
            notify(&mut tx, source_id).await?;
        }
        tx.commit().await?;
        Ok(replayed)
    }
}

/// Notifies [`CHANNEL`] that `source_id` has due work when `tx` commits.
async fn notify(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    source_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(CHANNEL)
        .bind(source_id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}
