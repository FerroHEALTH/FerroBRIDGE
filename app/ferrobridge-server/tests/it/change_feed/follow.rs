// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The follower loop: a feed that fails turns the consumer's indicator
//! degraded and keeps the process ready, a feed that answers turns it up,
//! the stop signal ends the loop, and only a full page that made progress is
//! read again without the idle wait.

use std::error::Error;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Duration;

use ferrobridge_server::change_feed::ChangeFeed;
use ferrobridge_server::change_feed::Domain;
use ferrobridge_server::change_feed::Event;
use ferrobridge_server::change_feed::FeedError;
use ferrobridge_server::change_feed::Page;
use ferrobridge_server::change_feed::PageSize;
use ferrobridge_server::change_feed::Position;
use ferrobridge_server::change_feed::ReaderName;
use ferrobridge_server::change_feed::Seq;
use ferrobridge_server::change_feed::ferroehr::FerroEhrFeed;
use ferrobridge_server::change_feed::follow::Polling;
use ferrobridge_server::change_feed::follow::follow;
use ferrobridge_server::change_feed::health::Following;
use ferrohealth_health::document::State;
use ferrohealth_health::lifecycle::Lifecycle;
use ferrohealth_health::registry::Registry;
use serde_json::json;
use tokio::time::Instant;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::Recording;
use super::client;
use super::reader;

/// The feed path of the test reader.
const FEED: &str = "/ext/admin/outbox/ferrobridge-test";

/// Returns polling that waits a millisecond between passes.
fn quick() -> Polling {
    Polling {
        page_size: PageSize::new(10).expect("a page size in range"),
        idle: Duration::from_millis(1),
        backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(2),
    }
}

/// Follows the feed `server` serves until `until` holds on the indicator,
/// then stops the follower.
async fn follow_until(
    server: &MockServer,
    until: impl Fn(&Following) -> bool,
) -> Result<Following, Box<dyn Error>> {
    let feed = FerroEhrFeed::new(client(server)?, format!("{}/ext", server.uri()).parse()?);
    let following = Following::new("change-feed-test");
    let consumer = Recording::default();
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let watched = following.clone();
    let name = reader();
    let follower = follow(&feed, &name, &consumer, quick(), None, &following, stopped);
    let waiter = async {
        for _ in 0..500 {
            if until(&watched) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        stop.send_replace(true);
    };
    tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(follower, waiter)
    })
    .await?;
    Ok(following)
}

#[tokio::test]
async fn a_failing_feed_is_degraded_and_the_process_stays_ready() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(FEED))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let following = follow_until(&server, |following| {
        following
            .outcome()
            .detail()
            .is_some_and(|detail| detail.contains("503"))
    })
    .await?;
    let outcome = following.outcome();
    assert_eq!(State::Degraded, outcome.state());
    let lifecycle = Lifecycle::default();
    lifecycle.booted();
    let readiness = Registry::new(vec![std::sync::Arc::new(following)])
        .readiness(&lifecycle)
        .await;
    assert!(
        readiness.is_ready(),
        "a degraded consumer never blocks readiness"
    );
    Ok(())
}

#[tokio::test]
async fn an_answering_feed_is_up() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(FEED))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "reader": "ferrobridge-test", "events": [] })),
        )
        .mount(&server)
        .await;
    let following = follow_until(&server, |following| {
        following.outcome().state() == State::Up
    })
    .await?;
    assert_eq!(State::Up, following.outcome().state());
    Ok(())
}

/// A clinical outbox in memory that records the instant of every read.
#[derive(Debug, Default)]
struct Clocked {
    /// The sequence numbers the outbox holds.
    seqs: Vec<u64>,
    /// The last acknowledged sequence number.
    cursor: Mutex<u64>,
    /// The instant of every read, in order.
    reads: Mutex<Vec<Instant>>,
}

impl Clocked {
    /// Returns the outbox holding `seqs`.
    fn holding(seqs: impl IntoIterator<Item = u64>) -> Self {
        Self {
            seqs: seqs.into_iter().collect(),
            ..Self::default()
        }
    }

    /// Returns the time from the first read to each later one.
    fn reads(&self) -> Vec<Duration> {
        let reads = self.reads.lock().unwrap_or_else(PoisonError::into_inner);
        let first = *reads.first().expect("the follower read at least once");
        reads.iter().map(|read| *read - first).collect()
    }
}

#[async_trait::async_trait]
impl ChangeFeed for Clocked {
    async fn page(&self, _reader: &ReaderName, limit: PageSize) -> Result<Page, FeedError> {
        self.reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Instant::now());
        let after = *self.cursor.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(Page {
            events: self
                .seqs
                .iter()
                .filter(|seq| **seq > after)
                .take(usize::from(limit.get()))
                .map(|seq| Event {
                    position: Position::Outbox {
                        domain: Domain::Clinical,
                        seq: Seq::new(*seq),
                    },
                    erasure: false,
                    contribution_id: None,
                    ehr_id: None,
                    committed_at: None,
                    versions: Vec::new(),
                    envelope: serde_json::Map::new(),
                })
                .collect(),
        })
    }

    async fn acknowledge(&self, _reader: &ReaderName, last: &Position) -> Result<(), FeedError> {
        let Position::Outbox { seq, .. } = last else {
            return Err(FeedError::Foreign {
                position: last.clone(),
            });
        };
        *self.cursor.lock().unwrap_or_else(PoisonError::into_inner) = seq.get();
        Ok(())
    }
}

/// Follows `feed` with pages of ten and a one-second idle wait for
/// `duration` of the paused clock, then stops the follower.
async fn follow_for(feed: &Clocked, duration: Duration) {
    let polling = Polling {
        idle: Duration::from_secs(1),
        ..quick()
    };
    let following = Following::new("change-feed-test");
    let consumer = Recording::default();
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let name = reader();
    let follower = follow(feed, &name, &consumer, polling, None, &following, stopped);
    let deadline = async {
        tokio::time::sleep(duration).await;
        stop.send_replace(true);
    };
    tokio::join!(follower, deadline);
}

#[tokio::test(start_paused = true)]
async fn an_empty_feed_is_polled_at_the_idle_interval() {
    let feed = Clocked::default();
    follow_for(&feed, Duration::from_millis(4_500)).await;
    let expected: Vec<Duration> = (0..5).map(Duration::from_secs).collect();
    assert_eq!(expected, feed.reads());
}

#[tokio::test(start_paused = true)]
async fn only_a_full_page_that_made_progress_is_read_again_at_once() {
    let feed = Clocked::holding(1..=25);
    follow_for(&feed, Duration::from_millis(2_500)).await;
    let at = |seconds| Duration::from_secs(seconds);
    assert_eq!(vec![at(0), at(0), at(0), at(1), at(2)], feed.reads());
}
