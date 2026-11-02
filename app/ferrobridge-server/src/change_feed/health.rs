// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The readiness indicator of one change-feed consumer.
//!
//! A consumer that cannot follow its feed (the CDR is down, the credentials
//! are refused, its own sink refuses) reports degraded, so the outage is
//! visible on the health surface while readiness holds: the events wait in
//! the CDR's outbox and nothing else the server serves depends on them. No
//! specification governs this: our own design.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use ferrohealth_health::registry::Check;
use ferrohealth_health::registry::HealthIndicator;
use ferrohealth_health::registry::Outcome;

/// Where one consumer stands, shared between its follower and its indicator.
#[derive(Debug, Clone)]
pub struct Following {
    /// The indicator name, one per consumer.
    name: &'static str,
    /// Why the last pass failed, or `None` after a pass that succeeded.
    state: Arc<Mutex<State>>,
}

/// The state of one consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    /// No pass has completed yet.
    Starting,
    /// The last pass completed.
    Following,
    /// The last pass failed, for the reason held.
    Stalled(String),
}

impl Following {
    /// Returns the state of the consumer reported as `name`, before its
    /// first pass.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            state: Arc::new(Mutex::new(State::Starting)),
        }
    }

    /// Records a pass that completed.
    pub fn followed(&self) {
        self.set(State::Following);
    }

    /// Records a pass that failed for `reason`, a line that carries no
    /// event body.
    pub fn stalled(&self, reason: impl Into<String>) {
        self.set(State::Stalled(reason.into()));
    }

    /// Returns what readiness reports for this consumer.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self.get() {
            State::Following => Outcome::up(),
            State::Starting => Outcome::degraded("the consumer has not read its feed yet"),
            State::Stalled(reason) => Outcome::degraded(reason),
        }
    }

    /// Replaces the state.
    fn set(&self, state: State) {
        // NOTE: no specification governs this: our own design; a poisoned
        // lock still holds a whole state, so it is written as it stands.
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = state;
    }

    /// Returns the state.
    fn get(&self) -> State {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
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
}
