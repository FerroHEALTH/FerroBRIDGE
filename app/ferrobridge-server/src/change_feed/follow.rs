// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One consumer following one feed: read a page after its reader's cursor,
//! hand each event to the consumer in order, and acknowledge the prefix the
//! consumer fully processed.
//!
//! Each reader is a lane with one leader across replicas, held by a lease
//! (`crate::store::lease`): only the leader reads, and every cursor write
//! carries its lease generation, so a former leader paused past its lease
//! moves no cursor and a new leader resumes at the first event the cursor
//! does not cover. The cursor moves only forward and only over events the
//! consumer finished: a failed read, a refused write or a consumer failure
//! leaves it where it was, and the event is read again (delivery is at least
//! once). No specification governs this: our own design.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::watch;

use crate::change_feed::ChangeFeed;
use crate::change_feed::Event;
use crate::change_feed::FeedError;
use crate::change_feed::Ledger;
use crate::change_feed::PageSize;
use crate::change_feed::Position;
use crate::change_feed::ReaderName;
use crate::change_feed::Stream;
use crate::change_feed::dead_letter::DeadLetterError;
use crate::change_feed::dead_letter::DeadLetters;
use crate::change_feed::health::Following;
use crate::store::lease::Leadership;
use crate::store::lease::Renewed;
use crate::store::lease::Term;

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

impl Failure {
    /// Returns the failure as one word of a sentence.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Transient => "transient",
            Self::Poisoned => "poisoned",
        }
    }
}

/// Why a consumer did not finish one event.
#[derive(Debug, thiserror::Error)]
#[error("the consumer met a {} failure", failure.word())]
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

    /// Finishes the work a pass shares across its events, once the pass
    /// offered its page and before it acknowledges any of it.
    ///
    /// # Errors
    /// Returns [`ConsumerError`] when the shared work did not finish; the
    /// pass then acknowledges nothing, so its events are offered again.
    async fn settle(&self) -> Result<(), ConsumerError> {
        Ok(())
    }
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

/// Where a follower parks an event its consumer failed with a poisoned
/// failure on `max_attempts` passes.
#[derive(Debug, Clone)]
pub struct Parking {
    /// The dead-letter store.
    pub store: DeadLetters,
    /// The poisoned passes an event fails before it is parked.
    pub max_attempts: NonZeroU32,
}

/// The last position this follower acknowledged in each stream, the guard
/// that keeps every cursor moving forward, and where it parks a poisoned
/// event.
#[derive(Debug, Clone, Default)]
pub struct Cursor {
    /// The last acknowledged event per stream, unknown until the stream's
    /// first acknowledgement.
    acknowledged: BTreeMap<Stream, Position>,
    /// Where a poisoned event is parked; without it a poisoned event stops
    /// the pass as a transient one does.
    parking: Option<Parking>,
    /// The positions this follower charged a poisoned pass to, forgotten
    /// once the event finishes.
    charged: Vec<Position>,
}

impl Cursor {
    /// Returns a cursor that parks a poisoned event through `parking`.
    #[must_use]
    pub const fn parking(parking: Parking) -> Self {
        Self {
            acknowledged: BTreeMap::new(),
            parking: Some(parking),
            charged: Vec::new(),
        }
    }

    /// Returns the last event this follower acknowledged in `stream`.
    #[must_use]
    pub fn acknowledged(&self, stream: Stream) -> Option<&Position> {
        self.acknowledged.get(&stream)
    }
}

/// What one pass did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pass {
    /// How many events the page held.
    pub read: usize,
    /// The positions the cursors were moved to, one per stream moved.
    pub acknowledged: Vec<Position>,
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
    #[error("the event at {position} was not finished")]
    Consumer {
        /// The event's position.
        position: Position,
        /// Why.
        #[source]
        source: ConsumerError,
    },
    /// The consumer did not finish the work its events share, so nothing of
    /// the page was acknowledged.
    #[error("the consumer did not settle the pass")]
    Settle {
        /// Why.
        #[source]
        source: ConsumerError,
    },
    /// A retried parked event met a transient failure; it stays parked
    /// with its retry pending.
    #[error("the replay of parked event {id} was not finished")]
    Replay {
        /// The parked number.
        id: u64,
        /// Why.
        #[source]
        source: ConsumerError,
    },
    /// The dead-letter store refused a read or a write.
    #[error("the dead-letter store refused the pass")]
    DeadLetter(#[from] DeadLetterError),
    /// The page holds an event that does not follow the one before it in
    /// its stream, or the cursor this follower moved there.
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
/// The page is checked whole before any event is offered: in each stream,
/// every event follows the one before it, and the first follows the cursor.
/// Each event is offered in page order until one fails; the prefix that
/// finished is then acknowledged, the last finished position of each stream,
/// and nothing after it.
///
/// The finished positions are written in one transaction fenced by `term`,
/// and a source that keeps its cursor on the CDR is told once it commits.
///
/// With [`Parking`], the parked events an operator decided to retry are
/// offered first, and a poisoned failure is charged to its event: on the
/// bound the event is parked with its cause and counts as finished, below it
/// the pass stops there. A transient failure is never charged.
///
/// # Errors
/// Returns [`PassError::Feed`] when the page cannot be read, the cursor
/// write is refused (a lost lease included) or an acknowledgement is refused, [`PassError::Unordered`] for a page out of
/// order, [`PassError::Replay`] for a replay that met a transient failure,
/// [`PassError::DeadLetter`] when the store refuses, and
/// [`PassError::Consumer`] for the first event the consumer did not finish,
/// after the prefix before it was acknowledged.
pub async fn pass(
    feed: &dyn ChangeFeed,
    reader: &ReaderName,
    consumer: &dyn Consumer,
    page_size: PageSize,
    cursor: &mut Cursor,
    term: &Term,
) -> Result<Pass, PassError> {
    let replayed = match cursor.parking.clone() {
        Some(parking) => replay(&parking.store, term, reader, consumer).await?,
        None => 0,
    };
    let page = feed.page(reader, page_size).await?;
    let mut previous: BTreeMap<Stream, &Position> = cursor
        .acknowledged
        .iter()
        .map(|(stream, position)| (*stream, position))
        .collect();
    for event in &page.events {
        let stream = event.position.stream();
        if let Some(before) = previous.get(&stream)
            && !event.position.follows(before)
        {
            return Err(PassError::Unordered {
                previous: Box::new((*before).clone()),
                position: Box::new(event.position.clone()),
            });
        }
        previous.insert(stream, &event.position);
    }
    let full = page.events.len() >= usize::from(page_size.get());
    let mut finished: BTreeMap<Stream, &Position> = BTreeMap::new();
    let mut failed = None;
    for event in &page.events {
        match consumer.process(event).await {
            Ok(()) => {
                forget(cursor, term, reader, &event.position).await?;
                finished.insert(event.position.stream(), &event.position);
            }
            Err(source) if parked(cursor, term, reader, event, &source).await? => {
                finished.insert(event.position.stream(), &event.position);
            }
            Err(source) => {
                failed = Some(PassError::Consumer {
                    position: event.position.clone(),
                    source,
                });
                break;
            }
        }
    }
    if (replayed > 0 || !finished.is_empty())
        && let Err(source) = consumer.settle().await
    {
        return Err(PassError::Settle { source });
    }
    let mut ledger = Ledger::new(term);
    for last in finished.values() {
        feed.record(&mut ledger, reader, last).await?;
    }
    ledger.commit().await?;
    let mut acknowledged = Vec::new();
    for (stream, last) in finished {
        feed.acknowledge(reader, last).await?;
        cursor.acknowledged.insert(stream, last.clone());
        acknowledged.push(last.clone());
    }
    match failed {
        Some(error) => Err(error),
        None => Ok(Pass {
            read: page.events.len(),
            acknowledged,
            full,
        }),
    }
}

/// Offers `consumer` every event parked for `reader` with a retry pending:
/// a finished one is settled, a poisoned one is parked again with its new
/// cause, and a transient failure stops the pass; returns how many it
/// offered.
async fn replay(
    store: &DeadLetters,
    term: &Term,
    reader: &ReaderName,
    consumer: &dyn Consumer,
) -> Result<usize, PassError> {
    let mut offered = 0_usize;
    for parked in store.parked(reader).await? {
        if !parked.replay {
            continue;
        }
        offered = offered.saturating_add(1);
        match consumer.process(&parked.event).await {
            Ok(()) => {
                store.replayed(term, reader, parked.id).await?;
                tracing::info!(consumer = consumer.name(), reader = %reader, parked = parked.id, "a replayed parked event finished");
            }
            Err(error) if error.failure() == Failure::Poisoned => {
                store
                    .repark(term, reader, parked.id, &crate::indicators::reason(&error))
                    .await?;
                tracing::warn!(consumer = consumer.name(), reader = %reader, parked = parked.id, "a replayed parked event failed again and stays parked");
            }
            Err(source) => {
                return Err(PassError::Replay {
                    id: parked.id,
                    source,
                });
            }
        }
    }
    Ok(offered)
}

/// Forgets the poisoned passes charged to the finished event at `position`.
async fn forget(
    cursor: &mut Cursor,
    term: &Term,
    reader: &ReaderName,
    position: &Position,
) -> Result<(), PassError> {
    let Some(parking) = cursor.parking.as_ref() else {
        return Ok(());
    };
    let before = cursor.charged.len();
    cursor.charged.retain(|charged| charged != position);
    if cursor.charged.len() != before {
        parking.store.forget(term, reader, position).await?;
    }
    Ok(())
}

/// Charges the poisoned failure `error` of `event` and parks the event on the
/// bound; returns whether it was parked.
async fn parked(
    cursor: &mut Cursor,
    term: &Term,
    reader: &ReaderName,
    event: &Event,
    error: &ConsumerError,
) -> Result<bool, PassError> {
    let Some(parking) = cursor.parking.as_ref() else {
        return Ok(false);
    };
    if error.failure() != Failure::Poisoned {
        return Ok(false);
    }
    let attempts = parking.store.charge(term, reader, &event.position).await?;
    if attempts < parking.max_attempts.get() {
        if !cursor.charged.contains(&event.position) {
            cursor.charged.push(event.position.clone());
        }
        return Ok(false);
    }
    let cause = crate::indicators::reason(error);
    let id = parking
        .store
        .park(term, reader, event, &cause, attempts)
        .await?;
    cursor.charged.retain(|charged| charged != &event.position);
    tracing::warn!(
        reader = %reader,
        position = %event.position,
        parked = id,
        attempts,
        "an event failed its bound of poisoned passes and is parked in the dead-letter store"
    );
    Ok(true)
}

/// Returns how long to wait after the pass `done`: nothing after a full page
/// that moved a cursor, so a backlog drains, and `idle` after any other, so
/// a feed that gives no progress is never read again at once.
fn wait_after(done: &Pass, idle: Duration) -> Duration {
    if done.full && !done.acknowledged.is_empty() {
        Duration::ZERO
    } else {
        idle
    }
}

/// What a follower needs besides its feed and its consumer.
#[derive(Debug, Clone)]
pub struct Lane {
    /// How the follower paces its reads.
    pub polling: Polling,
    /// Where a poisoned event is parked.
    pub parking: Option<Parking>,
    /// What readiness reports for the consumer.
    pub following: Following,
}

/// Follows the feed of `reader` with `consumer` while this replica leads the
/// reader's lane through `leadership`, until `stop` reads `true`.
///
/// A replica that does not lead waits a renewal period and claims again. A
/// leader renews its lease on its own task at the period `leadership` names,
/// and stops reading once the lease is lost or a fenced write is refused. A
/// full page that moved a cursor is followed by the next read at once, any
/// other pass after `lane.polling.idle`; a failed pass is logged, recorded on
/// `lane.following` and retried after a backoff that doubles up to
/// `lane.polling.max_backoff`. On `stop` the leader releases the lane.
pub async fn follow(
    feed: &dyn ChangeFeed,
    reader: &ReaderName,
    consumer: &dyn Consumer,
    leadership: Arc<dyn Leadership>,
    lane: Lane,
    mut stop: watch::Receiver<bool>,
) {
    let mut cursor = lane
        .parking
        .clone()
        .map_or_else(Cursor::default, Cursor::parking);
    let mut backoff = lane.polling.backoff;
    tracing::info!(consumer = consumer.name(), reader = %reader, "following the change feed");
    loop {
        if *stop.borrow() {
            break;
        }
        let wait = match leadership.claim().await {
            Ok(Some(term)) => {
                tracing::info!(consumer = consumer.name(), reader = %reader, generation = term.generation().get(), "this replica leads the change-feed reader");
                lead(
                    feed,
                    reader,
                    consumer,
                    &leadership,
                    &lane,
                    &term,
                    &mut cursor,
                    &mut stop,
                )
                .await;
                if *stop.borrow() {
                    if let Err(error) = leadership.release(&term).await {
                        tracing::warn!(consumer = consumer.name(), reader = %reader, reason = %crate::indicators::reason(&error), "the lease could not be released; it runs out on its own");
                    }
                    break;
                }
                backoff = lane.polling.backoff;
                lane.polling.backoff
            }
            Ok(None) => {
                lane.following.standby();
                leadership.renew_every()
            }
            Err(error) => {
                let reason = crate::indicators::reason(&error);
                tracing::warn!(consumer = consumer.name(), reader = %reader, reason = %reason, "the lease of the change-feed reader could not be claimed");
                lane.following.stalled(reason);
                let wait = backoff;
                backoff = backoff.saturating_mul(2).min(lane.polling.max_backoff);
                wait
            }
        };
        if !pause(&mut stop, wait).await {
            break;
        }
    }
    tracing::info!(consumer = consumer.name(), reader = %reader, "stopped following the change feed");
}

/// Waits `wait` unless `stop` turns `true` first; returns whether to go on.
async fn pause(stop: &mut watch::Receiver<bool>, wait: Duration) -> bool {
    if wait.is_zero() {
        return !*stop.borrow();
    }
    tokio::select! {
        _stopped = stop.wait_for(|stopped| *stopped) => false,
        () = tokio::time::sleep(wait) => true,
    }
}

/// Renews `term` every renewal period until it is lost, then flags `lost`.
async fn renew(leadership: Arc<dyn Leadership>, term: Term, lost: Arc<AtomicBool>) {
    loop {
        tokio::time::sleep(leadership.renew_every()).await;
        match leadership.renew(&term).await {
            Ok(Renewed::Held) => {}
            Ok(Renewed::Lost) => {
                lost.store(true, Ordering::Release);
                return;
            }
            Err(error) => tracing::warn!(
                lane = %term.lease().lane(),
                reason = %crate::indicators::reason(&error),
                "the lease could not be renewed; the next renewal tries again"
            ),
        }
    }
}

/// Follows the feed while `term` holds, until it is lost or `stop` reads
/// `true`.
#[expect(
    clippy::too_many_arguments,
    reason = "the follower's parts, each borrowed for the one term"
)]
async fn lead(
    feed: &dyn ChangeFeed,
    reader: &ReaderName,
    consumer: &dyn Consumer,
    leadership: &Arc<dyn Leadership>,
    lane: &Lane,
    term: &Term,
    cursor: &mut Cursor,
    stop: &mut watch::Receiver<bool>,
) {
    let lost = Arc::new(AtomicBool::new(false));
    let renewer = tokio::spawn(renew(
        Arc::clone(leadership),
        term.clone(),
        Arc::clone(&lost),
    ));
    cursor.acknowledged.clear();
    let mut backoff = lane.polling.backoff;
    let begun = feed.begin(reader, term).await;
    if let Err(error) = begun {
        lane.following.stalled(crate::indicators::reason(&error));
        renewer.abort();
        return;
    }
    loop {
        if *stop.borrow() || lost.load(Ordering::Acquire) {
            break;
        }
        let passed = pass(feed, reader, consumer, lane.polling.page_size, cursor, term).await;
        let positions: Vec<Position> = cursor.acknowledged.values().cloned().collect();
        lane.following.acknowledged(&positions);
        let wait = match passed {
            Ok(done) => {
                lane.following.followed();
                backoff = lane.polling.backoff;
                if done.read > 0 {
                    tracing::debug!(
                        consumer = consumer.name(),
                        reader = %reader,
                        read = done.read,
                        streams = done.acknowledged.len(),
                        "a change-feed pass finished"
                    );
                }
                wait_after(&done, lane.polling.idle)
            }
            Err(error) if is_lost(&error) => {
                tracing::warn!(consumer = consumer.name(), reader = %reader, "another replica took the change-feed reader; this one stops reading");
                break;
            }
            Err(error) => {
                let reason = crate::indicators::reason(&error);
                tracing::warn!(
                    consumer = consumer.name(),
                    reader = %reader,
                    reason = %reason,
                    "a change-feed pass failed; the cursor stays where it was"
                );
                lane.following.stalled(reason);
                let wait = backoff;
                backoff = backoff.saturating_mul(2).min(lane.polling.max_backoff);
                wait
            }
        };
        if !pause(stop, wait).await {
            break;
        }
    }
    renewer.abort();
}

/// Returns whether `error` says the lease is no longer this replica's.
fn is_lost(error: &PassError) -> bool {
    use crate::store::lease::LeaseError;
    matches!(
        error,
        PassError::Feed(FeedError::Lease(LeaseError::Stale { .. }))
            | PassError::DeadLetter(DeadLetterError::Lease(LeaseError::Stale { .. }))
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::Pass;
    use super::wait_after;
    use crate::change_feed::Domain;
    use crate::change_feed::Position;
    use crate::change_feed::Seq;

    const IDLE: Duration = Duration::from_secs(1);

    #[test]
    fn a_full_page_that_moved_a_cursor_is_read_again_at_once() {
        let done = Pass {
            read: 10,
            acknowledged: vec![Position::Outbox {
                domain: Domain::Clinical,
                seq: Seq::new(10),
            }],
            full: true,
        };
        assert_eq!(Duration::ZERO, wait_after(&done, IDLE));
    }

    #[test]
    fn a_full_page_that_moved_no_cursor_waits_the_idle_interval() {
        let done = Pass {
            read: 10,
            acknowledged: Vec::new(),
            full: true,
        };
        assert_eq!(IDLE, wait_after(&done, IDLE));
    }

    #[test]
    fn a_short_or_empty_page_waits_the_idle_interval() {
        let done = Pass {
            read: 0,
            acknowledged: Vec::new(),
            full: false,
        };
        assert_eq!(IDLE, wait_after(&done, IDLE));
    }
}
