// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The workers that apply inbound work, on every replica at once.
//!
//! A worker claims a due item, takes a slot of the item's source and tenant,
//! hands the item to the [`Handler`] with no transaction open, and gives the
//! item and the slot back. A notification on `ferrobridge_inbound` wakes an
//! idle worker; without one it looks again after the idle interval. No
//! specification governs an intake's workers: our own design.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Notify, watch};
use uuid::Uuid;

use super::store::{CHANNEL, Claimed, InboxStore, Outcome, Returned};
use crate::retry::{Failure, Next, Policy};
use crate::sources::{Source, Sources};
use crate::store::listen::{Dropping, Listened, Subscriber};

/// Why an attempt at a work item failed, in words that name no value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failed {
    /// Whether a repeat may succeed.
    pub failure: Failure,
    /// The fixed reason a parked item carries.
    pub reason: &'static str,
}

/// The future of one attempt.
pub type Attempt<'a> = Pin<Box<dyn Future<Output = Result<(), Failed>> + Send + 'a>>;

/// What a worker does with a claimed item: resolve the entity and apply it.
pub trait Handler: std::fmt::Debug + Send + Sync {
    /// Applies `item` of `source`.
    fn apply<'a>(&'a self, source: &'a Source, item: &'a Claimed) -> Attempt<'a>;
}

/// How the workers run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// The workers on this replica.
    pub workers: u32,
    /// How long a claimed item and a slot stay held.
    pub lease: Duration,
    /// The retry policy.
    pub policy: Policy,
    /// How long an idle worker waits before it looks again unwoken.
    pub idle: Duration,
}

/// The wake-up a notification on [`CHANNEL`] gives the idle workers.
#[derive(Debug, Default)]
struct Wake(Notify);

impl Subscriber for Wake {
    fn notified<'a>(&'a self, _payload: &'a str) -> Dropping<'a> {
        Box::pin(async move { self.0.notify_waiters() })
    }

    fn reset(&self) -> Dropping<'_> {
        Box::pin(async move { self.0.notify_waiters() })
    }
}

/// The workers of one replica.
#[derive(Debug)]
pub struct Pool {
    /// The rows.
    store: InboxStore,
    /// The sources, read per item.
    sources: Arc<Sources>,
    /// What applies an item.
    handler: Arc<dyn Handler>,
    /// How the workers run.
    settings: Settings,
    /// This replica, as the holder of its claims and slots.
    holder: Uuid,
    /// The wake-up.
    wake: Arc<Wake>,
    /// The listener state of [`CHANNEL`].
    listened: Arc<Listened>,
}

impl Pool {
    /// Returns the workers that apply the items of `store` with `handler`.
    #[must_use]
    pub fn new(
        store: InboxStore,
        sources: Arc<Sources>,
        handler: Arc<dyn Handler>,
        settings: Settings,
    ) -> Self {
        let wake = Arc::new(Wake::default());
        let listened = Arc::new(Listened::with_subscriber(
            CHANNEL,
            Arc::<Wake>::clone(&wake),
        ));
        Self {
            store,
            sources,
            handler,
            settings,
            holder: Uuid::new_v4(),
            wake,
            listened,
        }
    }

    /// Returns the listener state the replica's `LISTEN` session carries.
    #[must_use]
    pub fn listened(&self) -> Arc<Listened> {
        Arc::clone(&self.listened)
    }

    /// Claims one due item and applies it; `false` when none was due.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the store fails; the item's lease then
    /// runs out and another worker claims it.
    pub async fn step(&self) -> Result<bool, sqlx::Error> {
        let Some(item) = self
            .store
            .claim(self.holder, self.settings.lease, 1)
            .await?
            .into_iter()
            .next()
        else {
            return Ok(false);
        };
        let returned = self.attempt(&item).await?;
        if self.store.give_back(&item, self.holder, returned).await? == Outcome::Lost {
            tracing::warn!(
                source = %item.key.source_id,
                "an inbound item's lease ran out before its worker gave it back"
            );
        }
        Ok(true)
    }

    /// Attempts `item` under a slot of its source and tenant.
    async fn attempt(&self, item: &Claimed) -> Result<Returned, sqlx::Error> {
        let source = self.sources.get(item.key.source_id).await?;
        let Some(source) = source.filter(|source| source.enabled) else {
            return Ok(Returned::Defer(self.settings.policy.backoff.max));
        };
        let concurrency = match &source.parsed {
            crate::sources::document::Document::Webhook(webhook) => webhook.limits.concurrency,
            crate::sources::document::Document::FhirSubscription(_)
            | crate::sources::document::Document::Mllp(_) => 1,
        };
        let Some(slot) = self
            .store
            .take_slot(
                source.id,
                &item.key.tenant,
                concurrency,
                self.holder,
                self.settings.lease,
            )
            .await?
        else {
            return Ok(Returned::Defer(self.settings.policy.backoff.initial));
        };
        let applied = self.handler.apply(&source, item).await;
        self.store
            .release_slot(source.id, &item.key.tenant, slot, self.holder)
            .await?;
        Ok(match applied {
            Ok(()) => Returned::Done,
            Err(failed) => {
                // NOTE: our own design; a failed random source keeps the whole delay, never a shorter one.
                let random = crate::retry::random().unwrap_or(u32::MAX);
                match self.settings.policy.next(
                    failed.failure,
                    item.attempts.saturating_add(1),
                    random,
                ) {
                    Next::Retry(delay) => Returned::Retry(delay),
                    Next::Park => Returned::Park(failed.reason),
                }
            }
        })
    }

    /// Runs the workers on their own tasks until `stop` reads `true`.
    #[must_use]
    pub fn spawn(
        self: &Arc<Self>,
        stop: &watch::Receiver<bool>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        (0..self.settings.workers.max(1))
            .map(|_| {
                let pool = Arc::clone(self);
                let mut stop = stop.clone();
                tokio::spawn(async move {
                    while !*stop.borrow() {
                        let woken = pool.wake.0.notified();
                        match pool.step().await {
                            Ok(true) => continue,
                            Ok(false) => {}
                            Err(error) => {
                                tracing::warn!(%error, "the inbound store failed; the worker waits");
                            }
                        }
                        tokio::select! {
                            () = woken => {}
                            () = tokio::time::sleep(pool.settings.idle) => {}
                            _ = stop.changed() => {}
                        }
                    }
                })
            })
            .collect()
    }
}
