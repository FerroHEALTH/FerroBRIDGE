// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The state of each FHIR Subscription an inbound source notifies, and the
//! recovery of the events no notification delivered.
//!
//! A handshake records the subscription; every notification records its
//! status; each event becomes one work item for the patient it is about and
//! moves the highest event stored. An event count beyond the highest stored
//! event, from a heartbeat or a notification, is a gap, which `$events`
//! recovers (Subscriptions R5 Backport STU 1.1.0, errors.html) under the
//! source's recovery lease. No specification governs where the state is
//! kept: our own design.

use std::collections::BTreeMap;

use serde_json::Value;
use sqlx::postgres::PgPool;
use uuid::Uuid;

use super::event::{Change, DedupeKey, Event};
use super::store::{CursorError, store_event};
use super::subscription::{Notification, NotificationType, patient_of};
use crate::store::lease::{Generation, Lease};

/// What a notification did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Received {
    /// The source maps the subscription to no tenant; nothing was stored.
    Unknown,
    /// The notification was recorded.
    Recorded {
        /// The events stored as new.
        stored: usize,
        /// The gap still to recover, when there is one.
        gap: Option<(u64, u64)>,
    },
}

/// A gap of one subscription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    /// The subscription reference.
    pub subscription: String,
    /// The tenant it serves.
    pub tenant: String,
    /// The first missing event number.
    pub from: u64,
    /// The last missing event number.
    pub to: u64,
}

/// The subscription state rows.
#[derive(Debug, Clone)]
pub struct Keeper {
    /// The state store's pool.
    pool: PgPool,
}

/// Returns `value` as a non-negative `bigint`, saturating.
fn bigint(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

impl Keeper {
    /// Returns the state rows of the store behind `pool`.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Records `notification` of `source_id`, whose tenants by subscription
    /// are `subscriptions`, in one transaction: the status, each event as
    /// one work item, the highest event and the gap.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when a statement or the commit fails; nothing
    /// is recorded then.
    pub async fn receive(
        &self,
        source_id: Uuid,
        subscriptions: &BTreeMap<String, String>,
        notification: &Notification,
    ) -> Result<Received, sqlx::Error> {
        let Some(tenant) = subscriptions.get(&notification.subscription) else {
            return Ok(Received::Unknown);
        };
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO fhir_subscription_state (source_id, subscription, tenant, status) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (source_id, subscription) DO NOTHING",
        )
        .bind(source_id)
        .bind(&notification.subscription)
        .bind(tenant)
        .bind(&notification.status)
        .execute(&mut *tx)
        .await?;
        let (last, gap_from, gap_to): (i64, Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT last_event, gap_from, gap_to FROM fhir_subscription_state \
             WHERE source_id = $1 AND subscription = $2 FOR UPDATE",
        )
        .bind(source_id)
        .bind(&notification.subscription)
        .fetch_one(&mut *tx)
        .await?;
        let last = last.unsigned_abs();
        let mut gap = gap_from
            .zip(gap_to)
            .map(|(from, to)| (from.unsigned_abs(), to.unsigned_abs()));
        let mut widen = |from: u64, to: u64| {
            if from <= to {
                gap = Some(gap.map_or((from, to), |(a, b)| (a.min(from), b.max(to))));
            }
        };
        let mut stored = 0;
        let mut highest = last;
        for event in &notification.events {
            let entity = patient_of(event).or_else(|| event.focus.clone());
            let Some(entity) = entity else {
                widen(event.number, event.number);
                continue;
            };
            let accepted = store_event(
                &mut tx,
                source_id,
                tenant,
                &Event {
                    change: Change::Upsert,
                    tenant: Some(tenant.clone()),
                    entity,
                    dedupe_key: DedupeKey::of(vec![
                        Value::String(notification.subscription.clone()),
                        Value::from(event.number),
                    ]),
                    stamp: Some(event.number.to_string()),
                },
            )
            .await?;
            if accepted == super::store::Accepted::Stored {
                stored += 1;
            }
            highest = highest.max(event.number);
        }
        let numbers: Vec<u64> = notification
            .events
            .iter()
            .map(|event| event.number)
            .collect();
        if let Some(lowest) = numbers.iter().copied().min() {
            widen(last.saturating_add(1), lowest.saturating_sub(1));
        }
        if let Some(since) = notification.events_since_start {
            widen(highest.saturating_add(1), since);
            if notification.kind == NotificationType::Handshake {
                highest = highest.max(since);
            }
        }
        let carried = u64::try_from(numbers.len()).unwrap_or(u64::MAX);
        if gap.is_some_and(|(from, to)| {
            to.saturating_sub(from) < carried && (from..=to).all(|number| numbers.contains(&number))
        }) {
            gap = None;
        }
        sqlx::query(
            "UPDATE fhir_subscription_state SET status = $3, last_event = $4, \
               gap_from = $5, gap_to = $6, updated_at = now(), \
               handshake_at = CASE WHEN $7 THEN now() ELSE handshake_at END \
             WHERE source_id = $1 AND subscription = $2",
        )
        .bind(source_id)
        .bind(&notification.subscription)
        .bind(&notification.status)
        .bind(bigint(highest))
        .bind(gap.map(|(from, _)| bigint(from)))
        .bind(gap.map(|(_, to)| bigint(to)))
        .bind(notification.kind == NotificationType::Handshake)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Received::Recorded { stored, gap })
    }

    /// Returns the gaps of `source_id`'s subscriptions.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the statement fails.
    pub async fn gaps(&self, source_id: Uuid) -> Result<Vec<Gap>, sqlx::Error> {
        let rows: Vec<(String, String, i64, i64)> = sqlx::query_as(
            "SELECT subscription, tenant, gap_from, gap_to FROM fhir_subscription_state \
             WHERE source_id = $1 AND gap_from IS NOT NULL ORDER BY subscription",
        )
        .bind(source_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(subscription, tenant, from, to)| Gap {
                subscription,
                tenant,
                from: from.unsigned_abs(),
                to: to.unsigned_abs(),
            })
            .collect())
    }

    /// Closes `gap` of `source_id` once `$events` recovered it, refused
    /// unless `generation` still holds `lease`.
    ///
    /// # Errors
    /// Returns [`CursorError::Fenced`] when another replica took the lease,
    /// and [`CursorError::Database`] when a statement fails.
    pub async fn close(
        &self,
        lease: &Lease,
        generation: Generation,
        source_id: Uuid,
        gap: &Gap,
    ) -> Result<(), CursorError> {
        let mut tx = self.pool.begin().await?;
        lease.fence(&mut tx, generation).await?;
        sqlx::query(
            "UPDATE fhir_subscription_state SET \
               gap_from = CASE WHEN gap_to > $4 THEN $4 + 1 END, \
               gap_to = CASE WHEN gap_to > $4 THEN gap_to END, \
               last_event = greatest(last_event, $4), updated_at = now() \
             WHERE source_id = $1 AND subscription = $2 AND gap_from = $3",
        )
        .bind(source_id)
        .bind(&gap.subscription)
        .bind(bigint(gap.from))
        .bind(bigint(gap.to))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
