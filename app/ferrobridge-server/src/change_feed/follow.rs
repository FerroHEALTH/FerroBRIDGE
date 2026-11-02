// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One consumer following one feed: read a page after its reader's cursor,
//! hand each event to the consumer in order, and acknowledge the prefix the
//! consumer fully processed.
//!
//! The reader's cursor on the CDR is the consumer's durable watermark, so a
//! restart resumes at the first event it had not finished. The cursor moves
//! only by an acknowledgement, only forward, and only over events the
//! consumer finished: a failed read, a refused acknowledgement or a consumer
//! failure leaves it where it was, and the event is read again (delivery is
//! at least once). No specification governs this: our own design.

use std::time::Duration;

use tokio::sync::watch;

use crate::change_feed::ChangeFeed;
use crate::change_feed::Event;
use crate::change_feed::FeedError;
use crate::change_feed::PageSize;
use crate::change_feed::Position;
use crate::change_feed::ReaderName;
use crate::change_feed::health::Following;

/// Whether a failed event may succeed when it is offered again unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// A failure of something the event depends on (an unreachable sink, a
    /// timeout): the event is offered again on the next pass.
    Transient,
    /// A failure of the event itself (content no mapping admits): offering it
    /// again fails the same way.
    Poisoned,
}

/// Why a consumer did not finish one event.
#[derive(Debug, thiserror::Error)]
#[error("the consumer did not finish the event ({failure:?})")]
pub struct ConsumerError {
    /// Whether offering the event again may succeed.
    failure: Failure,
    /// What failed.
    #[source]
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl ConsumerError {
    /// Returns a failure that may pass when the event is offered again.
    #[must_use]
    pub fn transient(source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self {
            failure: Failure::Transient,
            source: source.into(),
        }
    }

    /// Returns a failure of the event itself.
    #[must_use]
    pub fn poisoned(source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self {
            failure: Failure::Poisoned,
            source: source.into(),
        }
    }

    /// Returns whether offering the event again may succeed.
    #[must_use]
    pub const fn failure(&self) -> Failure {
        self.failure
    }
}

/// What one feed consumer does with each event, in `seq` order.
#[async_trait::async_trait]
pub trait Consumer: std::fmt::Debug + Send + Sync {
    /// Returns the name readiness and the logs report the consumer under.
    fn name(&self) -> &'static str;

    /// Finishes `event`: when this returns `Ok`, the event may be
    /// acknowledged and is never offered again.
    ///
    /// # Errors
    /// Returns [`ConsumerError`] when the event is not finished.
    async fn process(&self, event: &Event) -> Result<(), ConsumerError>;
}

/// How a follower paces its reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Polling {
    /// How many events one read asks for.
    pub page_size: PageSize,
    /// How long to wait after a page that was not full.
    pub idle: Duration,
    /// How long to wait after the first failed pass, doubled per failure.
    pub backoff: Duration,
    /// The longest wait after a failed pass.
    pub max_backoff: Duration,
}

impl Default for Polling {
    fn default() -> Self {
        Self {
            page_size: PageSize::default(),
            idle: Duration::from_secs(1),
            backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
        }
    }
}

/// The last position this follower acknowledged, the guard that keeps its
/// cursor moving forward.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cursor {
    /// The last acknowledged event, unknown until the first acknowledgement.
    acknowledged: Option<Position>,
}

impl Cursor {
    /// Returns the last event this follower acknowledged.
    #[must_use]
    pub const fn acknowledged(&self) -> Option<&Position> {
        self.acknowledged.as_ref()
    }
}

/// What one pass did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pass {
    /// How many events the page held.
    pub read: usize,
    /// The event the cursor was moved to, when the pass moved it.
    pub acknowledged: Option<Position>,
    /// Whether the page was full, so more events may wait.
    pub full: bool,
}

/// Why a pass stopped before the end of its page.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PassError {
    /// The feed did not answer a page, or refused the acknowledgement.
    #[error("the change feed refused the pass")]
    Feed(#[from] FeedError),
    /// The consumer did not finish an event.
    #[error("the consumer did not finish the event at {position}")]
    Consumer {
        /// The event's position.
        position: Position,
        /// Why.
        #[source]
        source: ConsumerError,
    },
    /// The page holds an event that does not follow the one before it, or
    /// the cursor this follower moved.
    #[error("the feed served the event at {position} after {previous}")]
    Unordered {
        /// The position before it.
        previous: Box<Position>,
        /// The position out of order.
        position: Box<Position>,
    },
}

/// Runs one pass of `consumer` over the feed of `reader`.
///
/// The page is checked whole before any event is offered: every event
/// follows the one before it, and the first follows `cursor`. Each event is
/// offered in order until one fails; the prefix that finished is then
/// acknowledged, and nothing after it.
///
/// # Errors
/// Returns [`PassError::Feed`] when the page cannot be read or the
/// acknowledgement is refused, [`PassError::Unordered`] for a page out of
/// order, and [`PassError::Consumer`] for the first event the consumer did
/// not finish, after the prefix before it was acknowledged.
pub async fn pass(
    feed: &dyn ChangeFeed,
    reader: &ReaderName,
    consumer: &dyn Consumer,
    page_size: PageSize,
    cursor: &mut Cursor,
) -> Result<Pass, PassError> {
    let page = feed.page(reader, page_size).await?;
    let mut previous = cursor.acknowledged.as_ref();
    for event in &page.events {
        if let Some(before) = previous
            && !event.position.follows(before)
        {
            return Err(PassError::Unordered {
                previous: Box::new(before.clone()),
                position: Box::new(event.position.clone()),
            });
        }
        previous = Some(&event.position);
    }
    let full = page.events.len() >= usize::from(page_size.get());
    let mut finished = None;
    let mut failed = None;
    for event in &page.events {
        match consumer.process(event).await {
            Ok(()) => finished = Some(&event.position),
            Err(source) => {
                failed = Some(PassError::Consumer {
                    position: event.position.clone(),
                    source,
                });
                break;
            }
        }
    }
    if let Some(last) = finished {
        feed.acknowledge(reader, last).await?;
        cursor.acknowledged = Some(last.clone());
    }
    match failed {
        Some(error) => Err(error),
        None => Ok(Pass {
            read: page.events.len(),
            acknowledged: finished.cloned(),
            full,
        }),
    }
}

/// Follows the feed of `reader` with `consumer` until `stop` reads `true`.
///
/// A full page is followed by the next read at once, a short one after
/// `polling.idle`; a failed pass is logged, recorded on `following` and
/// retried after a backoff that doubles up to `polling.max_backoff`.
pub async fn follow(
    feed: &dyn ChangeFeed,
    reader: &ReaderName,
    consumer: &dyn Consumer,
    polling: Polling,
    following: &Following,
    mut stop: watch::Receiver<bool>,
) {
    let mut cursor = Cursor::default();
    let mut backoff = polling.backoff;
    tracing::info!(consumer = consumer.name(), reader = %reader, "following the change feed");
    loop {
        if *stop.borrow() {
            break;
        }
        let wait = match pass(feed, reader, consumer, polling.page_size, &mut cursor).await {
            Ok(done) => {
                following.followed();
                backoff = polling.backoff;
                if done.read > 0 {
                    tracing::debug!(
                        consumer = consumer.name(),
                        reader = %reader,
                        read = done.read,
                        acknowledged = done.acknowledged.as_ref().map(ToString::to_string),
                        "a change-feed pass finished"
                    );
                }
                if done.full {
                    Duration::ZERO
                } else {
                    polling.idle
                }
            }
            Err(error) => {
                let reason = crate::indicators::reason(&error);
                tracing::warn!(
                    consumer = consumer.name(),
                    reader = %reader,
                    reason = %reason,
                    "a change-feed pass failed; the cursor stays where it was"
                );
                following.stalled(reason);
                let wait = backoff;
                backoff = backoff.saturating_mul(2).min(polling.max_backoff);
                wait
            }
        };
        if wait.is_zero() {
            continue;
        }
        tokio::select! {
            stopped = stop.wait_for(|stopped| *stopped) => {
                if stopped.is_err() {
                    break;
                }
            }
            () = tokio::time::sleep(wait) => {}
        }
    }
    tracing::info!(consumer = consumer.name(), reader = %reader, "stopped following the change feed");
}
