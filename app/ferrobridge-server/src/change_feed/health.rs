// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where one change-feed consumer stands: its readiness indicator, and the
//! status the operator API reports.
//!
//! A consumer that cannot follow its feed (the CDR is down, the credentials
//! are refused, its own sink refuses) reports degraded, so the outage is
//! visible on the health surface while readiness holds: the events wait in
//! the CDR's outbox and nothing else the server serves depends on them. The
//! status beside it keeps the position of each stream, what the consumer
//! delivered since start and the last failure with its cause, never an event
//! body. No specification governs this: our own design.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use ferrohealth_health::registry::Check;
use ferrohealth_health::registry::HealthIndicator;
use ferrohealth_health::registry::Outcome;

use crate::change_feed::Position;
use crate::change_feed::Stream;

/// Where one consumer stands, shared between its follower, its consumer,
/// its indicator and the operator API.
#[derive(Debug, Clone)]
pub struct Following {
    /// The indicator name, one per consumer.
    name: &'static str,
    /// The status.
    status: Arc<Mutex<Status>>,
}

/// The state of one consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// No pass has completed yet.
    Starting,
    /// The last pass completed.
    Following,
    /// The last pass failed, for the reason held.
    Stalled(String),
}

/// A failed pass: when, and the failure with its causes in one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// When the pass failed.
    pub at: jiff::Timestamp,
    /// The failure and its causes.
    pub cause: String,
}

/// What a consumer's status holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// Where the consumer stands.
    pub state: State,
    /// The last position acknowledged in each stream.
    pub positions: BTreeMap<Stream, Position>,
    /// How many messages the consumer delivered since start.
    pub delivered: u64,
    /// When it last delivered one.
    pub last_delivered: Option<jiff::Timestamp>,
    /// The last failed pass, kept after the consumer recovers.
    pub last_failure: Option<Failure>,
}

impl Following {
    /// Returns the state of the consumer reported as `name`, before its
    /// first pass.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            status: Arc::new(Mutex::new(Status {
                state: State::Starting,
                positions: BTreeMap::new(),
                delivered: 0,
                last_delivered: None,
                last_failure: None,
            })),
        }
    }

    /// Records a pass that completed.
    pub fn followed(&self) {
        self.with(|status| status.state = State::Following);
    }

    /// Records a pass that failed for `reason`, a line that carries no
    /// event body.
    pub fn stalled(&self, reason: impl Into<String>) {
        let cause = reason.into();
        self.with(|status| {
            status.last_failure = Some(Failure {
                at: jiff::Timestamp::now(),
                cause: cause.clone(),
            });
            status.state = State::Stalled(cause);
        });
    }

    /// Records `positions` as the last acknowledged position of each of
    /// their streams.
    pub fn acknowledged(&self, positions: &[Position]) {
        self.with(|status| {
            for position in positions {
                status.positions.insert(position.stream(), position.clone());
            }
        });
    }

    /// Records one message the consumer delivered.
    pub fn delivered(&self) {
        self.with(|status| {
            status.delivered = status.delivered.saturating_add(1);
            status.last_delivered = Some(jiff::Timestamp::now());
        });
    }

    /// Returns the status as it stands.
    #[must_use]
    pub fn status(&self) -> Status {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Returns what readiness reports for this consumer.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self.status().state {
            State::Following => Outcome::up(),
            State::Starting => Outcome::degraded("the consumer has not read its feed yet"),
            State::Stalled(reason) => Outcome::degraded(reason),
        }
    }

    /// Changes the status with `change`.
    fn with(&self, change: impl FnOnce(&mut Status)) {
        // NOTE: no specification governs this: our own design; a poisoned
        // lock still holds a whole status, so it is changed as it stands.
        change(&mut self.status.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

impl HealthIndicator for Following {
    fn name(&self) -> &'static str {
        self.name
    }

    fn check(&self) -> Check<'_> {
        Box::pin(async move { self.outcome() })
    }
}

#[cfg(test)]
mod tests {
    use super::Following;
    use ferrohealth_health::document::State;

    #[test]
    fn a_consumer_is_degraded_until_a_pass_completes_and_again_after_one_fails() {
        let following = Following::new("change-feed-test");
        assert_eq!(State::Degraded, following.outcome().state());
        following.followed();
        assert_eq!(State::Up, following.outcome().state());
        following.stalled("the feed answered 503 Service Unavailable");
        let outcome = following.outcome();
        assert_eq!(State::Degraded, outcome.state());
        assert_eq!(
            Some("the feed answered 503 Service Unavailable"),
            outcome.detail()
        );
    }

    #[test]
    fn the_last_failure_outlives_the_recovery() {
        let following = Following::new("change-feed-test");
        following.stalled("the broker could not be reached");
        following.followed();
        following.delivered();
        let status = following.status();
        assert_eq!(super::State::Following, status.state);
        assert_eq!(1, status.delivered);
        assert_eq!(
            Some("the broker could not be reached"),
            status
                .last_failure
                .as_ref()
                .map(|failure| failure.cause.as_str())
        );
    }
}
