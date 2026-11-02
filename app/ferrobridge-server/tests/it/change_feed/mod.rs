// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The change-feed adapter against stub CDRs: the vendor-neutral AQL source,
//! the reference CDR's outbox feed, and the follower that moves a reader's
//! cursor only over what its consumer finished.
//!
//! Every body is synthetic: identifiers and instants invented for the test.

mod aql;
mod dead_letter;
mod ferroehr;
mod follow;

use std::error::Error;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Duration;

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::change_feed::Event;
use ferrobridge_server::change_feed::Position;
use ferrobridge_server::change_feed::ReaderName;
use ferrobridge_server::change_feed::follow::Consumer;
use ferrobridge_server::change_feed::follow::ConsumerError;
use openehr_its::rest::client::RetryPolicy;
use wiremock::MockServer;

/// The reader every case reads under.
pub(crate) fn reader() -> ReaderName {
    ReaderName::new("ferrobridge-test").expect("a valid reader name")
}

/// Returns a CDR client against `server`, with basic credentials (a password
/// drawn per run, so no credential is written down) and one
/// attempt per call.
pub(crate) fn client(server: &MockServer) -> Result<CdrClient, Box<dyn Error>> {
    let config = CdrConfig::new(format!("{}/v1", server.uri()).parse()?)
        .with_timeout(Duration::from_secs(5))
        .with_retry(RetryPolicy {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
        })
        .with_basic("bridge", uuid::Uuid::new_v4().to_string());
    Ok(CdrClient::new(&config)?)
}

/// A consumer that records the positions it finished and fails on the
/// event at a chosen index.
#[derive(Debug, Default)]
pub(crate) struct Recording {
    /// The positions finished, in order.
    finished: Mutex<Vec<Position>>,
    /// The index of the event to fail, counted over every offer.
    fail_at: Option<usize>,
    /// How many events were offered.
    offered: Mutex<usize>,
}

impl Recording {
    /// Returns a consumer that fails the event offered at `index`.
    pub(crate) fn failing_at(index: usize) -> Self {
        Self {
            fail_at: Some(index),
            ..Self::default()
        }
    }

    /// Returns the positions finished.
    pub(crate) fn finished(&self) -> Vec<Position> {
        self.finished
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait::async_trait]
impl Consumer for Recording {
    fn name(&self) -> &'static str {
        "change-feed-test"
    }

    async fn process(&self, event: &Event) -> Result<(), ConsumerError> {
        let index = {
            let mut offered = self.offered.lock().unwrap_or_else(PoisonError::into_inner);
            let index = *offered;
            *offered += 1;
            index
        };
        if self.fail_at == Some(index) {
            return Err(ConsumerError::transient("the synthetic sink is down"));
        }
        self.finished
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event.position.clone());
        Ok(())
    }
}
