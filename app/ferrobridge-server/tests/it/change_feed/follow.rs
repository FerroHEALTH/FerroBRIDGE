// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The follower loop: a feed that fails turns the consumer's indicator
//! degraded and keeps the process ready, a feed that answers turns it up,
//! and the stop signal ends the loop.

use std::error::Error;
use std::time::Duration;

use ferrobridge_server::change_feed::PageSize;
use ferrobridge_server::change_feed::ferroehr::FerroEhrFeed;
use ferrobridge_server::change_feed::follow::Polling;
use ferrobridge_server::change_feed::follow::follow;
use ferrobridge_server::change_feed::health::Following;
use ferrohealth_health::document::State;
use ferrohealth_health::lifecycle::Lifecycle;
use ferrohealth_health::registry::Registry;
use serde_json::json;
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
    let follower = follow(&feed, &name, &consumer, quick(), &following, stopped);
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
