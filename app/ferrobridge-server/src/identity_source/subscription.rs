// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The deployment's one PMIR subscription to the identity source's feed.
//!
//! One replica holds the `pmir-subscription` lane lease. Every period it
//! adopts the subscription it created before, or creates one (PMIR 1.6.0
//! ITI-94, §2:3.94.4.1), reads its status (§2:3.94.4.3), records the status
//! in the `pmir_subscription` row under its lease, and announces it on
//! [`crate::store::listen::PSEUDONYMS`], so every replica's pseudonym cache
//! serves only while the feed is `active`. A status it could not read is
//! announced as `unreachable`, which closes every cache. No specification
//! governs which replica subscribes: our own design.

use std::sync::Arc;
use std::time::Duration;

use http::StatusCode;
use ihe_iti::pmir::PmirSubscriber;
use ihe_iti::pmir::error::SubscribeError;
use ihe_iti::pmir::subscription::{Search, Subscribed, SubscriptionRequest};
use sqlx::postgres::PgPool;

use super::cache::PseudonymCache;
use crate::store::lease::{Kept, Lane, Lease, LeaseError, Term, spawn_led};

/// The lane whose lease the subscription's holder takes.
pub const LANE: &str = "pmir-subscription";

/// The status announced when the identity source could not be asked.
const UNREACHABLE: &str = "unreachable";

/// Returns whether the subscription's row says `active` and was written
/// within `within`.
///
/// # Errors
/// Returns [`sqlx::Error`] when the statement fails.
pub(super) async fn recorded_active(pool: &PgPool, within: Duration) -> Result<bool, sqlx::Error> {
    // NOTE: no specification governs this: our own design; a period past i64
    // milliseconds is read as the longest the statement binds.
    let within = i64::try_from(within.as_millis()).unwrap_or(i64::MAX);
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pmir_subscription WHERE status = $1 \
         AND checked_at > clock_timestamp() - $2 * interval '1 millisecond')",
    )
    .bind(super::cache::ACTIVE)
    .bind(within)
    .fetch_one(pool)
    .await
}

/// The holder of the subscription, run by whichever replica holds its lease.
#[derive(Clone)]
pub struct Holder {
    /// The ITI-94 client of the identity source.
    subscriber: PmirSubscriber,
    /// The subscription this deployment asks for.
    request: SubscriptionRequest,
    /// The cache its status is announced to.
    cache: PseudonymCache,
    /// The lease on [`LANE`].
    lease: Lease,
    /// The pool the row is written through.
    pool: PgPool,
    /// How often the status is read.
    every: Duration,
    /// How long one exchange with the identity source may take.
    timeout: Duration,
}

impl std::fmt::Debug for Holder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Holder")
            .field("lane", &LANE)
            .field("every", &self.every)
            .finish_non_exhaustive()
    }
}

/// What one holder's period needs beside the lease.
#[derive(Clone)]
pub struct Subscription {
    /// The ITI-94 client of the identity source.
    pub subscriber: PmirSubscriber,
    /// The subscription this deployment asks for.
    pub request: SubscriptionRequest,
    /// How often the status is read.
    pub every: Duration,
    /// How long one exchange may take.
    pub timeout: Duration,
}

impl std::fmt::Debug for Subscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscription")
            .field("every", &self.every)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl Holder {
    /// Returns the holder of `subscription`, announcing to `cache`, leasing
    /// [`LANE`] from `store` for three periods at a time.
    ///
    /// # Errors
    /// Returns [`LeaseError::Lane`] only for an empty lane name, which
    /// [`LANE`] is not.
    pub fn new(
        subscription: Subscription,
        cache: PseudonymCache,
        store: &crate::store::Store,
    ) -> Result<Self, LeaseError> {
        let lease = store.lease(Lane::new(LANE)?, subscription.every.saturating_mul(3));
        Ok(Self {
            subscriber: subscription.subscriber,
            request: subscription.request,
            cache,
            lease,
            pool: store.pool().clone(),
            every: subscription.every,
            timeout: subscription.timeout,
        })
    }

    /// Runs the holder on its own task every period until `stop` reads
    /// `true`, then gives the lease up.
    #[must_use]
    pub fn spawn(self, stop: &tokio::sync::watch::Receiver<bool>) -> tokio::task::JoinHandle<()> {
        let known = Arc::new(tokio::sync::Mutex::new(None));
        let lease = self.lease.clone();
        let every = self.every;
        spawn_led(lease, every, LANE, stop, move |term| {
            let holder = self.clone();
            let known = Arc::clone(&known);
            async move {
                let mut known = known.lock().await;
                holder.period(&term, &mut known).await
            }
        })
    }

    /// Reads, records and announces the subscription's status under `term`.
    async fn period(&self, term: &Term, known: &mut Option<Subscribed>) -> Kept {
        let status = match self.status(known).await {
            Ok(status) => status,
            Err(error) => {
                tracing::warn!(error = %ferrohealth_server::run::chain(&error), "the PMIR subscription's status could not be read; every pseudonym cache asks the identity source");
                UNREACHABLE
            }
        };
        let kept = match self.record(term, known.as_ref(), status).await {
            Ok(()) => Kept::Held,
            Err(LeaseError::Stale { .. }) => Kept::Lost,
            Err(error) => {
                tracing::warn!(error = %ferrohealth_server::run::chain(&error), "the PMIR subscription's status could not be recorded");
                Kept::Held
            }
        };
        if let Err(error) = self.cache.announce(status).await {
            tracing::warn!(error = %ferrohealth_server::run::chain(&error), "the PMIR subscription's status could not be announced");
        }
        kept
    }

    /// Returns the status of the subscription `known` locates, adopting or
    /// creating one first when none is known.
    async fn status(&self, known: &mut Option<Subscribed>) -> Result<&'static str, SubscribeError> {
        let subscribed = match known.take() {
            Some(subscribed) => subscribed,
            None => self.adopt_or_create().await?,
        };
        match self.subscriber.status(&subscribed, self.timeout).await {
            Ok(status) => {
                *known = Some(subscribed);
                Ok(status.as_str())
            }
            // NOTE: PMIR 1.6.0 §2:3.94.4.3 and FHIR R4 http.html read: a 404 or 410 is a
            // subscription the Registry no longer holds, so the next period creates one.
            Err(SubscribeError::Rejected { status, .. })
                if status == StatusCode::NOT_FOUND || status == StatusCode::GONE =>
            {
                Ok(UNREACHABLE)
            }
            Err(error) => {
                *known = Some(subscribed);
                Err(error)
            }
        }
    }

    /// Returns the subscription this deployment created before, found by
    /// its channel endpoint, or a new one.
    async fn adopt_or_create(&self) -> Result<Subscribed, SubscribeError> {
        if let Search::Found(listed) = self.subscriber.find(&self.request, self.timeout).await?
            && let Some(first) = listed.into_iter().next()
        {
            return Ok(first.subscribed);
        }
        self.subscriber.subscribe(&self.request, self.timeout).await
    }

    /// Records `status` of the subscription at `location` under `term`.
    async fn record(
        &self,
        term: &Term,
        location: Option<&Subscribed>,
        status: &str,
    ) -> Result<(), LeaseError> {
        let mut transaction = self.pool.begin().await?;
        term.fence(&mut transaction).await?;
        sqlx::query(
            "INSERT INTO pmir_subscription (singleton, location, status, checked_at, generation) \
             VALUES (true, $1, $2, clock_timestamp(), $3) ON CONFLICT (singleton) DO UPDATE SET \
             location = EXCLUDED.location, status = EXCLUDED.status, \
             checked_at = EXCLUDED.checked_at, generation = EXCLUDED.generation",
        )
        .bind(location.map(|subscribed| subscribed.location().as_str()))
        .bind(status)
        .bind(term.generation().get())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }
}
