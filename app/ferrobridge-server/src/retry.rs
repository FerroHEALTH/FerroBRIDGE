// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The wait and the verdict of the bridge's own loops.
//!
//! The [`Backoff`] spaces two attempts, the [`Failure`] says whether a repeat
//! may succeed, the [`Statuses`] read a failure from an HTTP answer, the
//! [`Policy`] turns a failure and an attempt count into a retry or a park,
//! and the [`Decided`] record is an operator's settlement of parked work, in
//! the change feed's dead letters and the inbound engine's work items alike.
//!
//! Each loop keeps its own numbers and its own park rule: the start and the
//! change-feed follower double a wait up to a ceiling, and the inbound
//! workers jitter it and park after a bound. The retry of an upstream client
//! (the generated ITS-REST client, `ferrobridge-term`, `ferrohealth-smart`)
//! belongs to its crate. No specification governs this: our own design.

use std::fmt;
use std::time::Duration;

use http::StatusCode;

/// A wait that doubles from `initial` up to `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    /// The wait after the first failed attempt.
    pub initial: Duration,
    /// The longest wait.
    pub max: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(30),
        }
    }
}

impl Backoff {
    /// Returns the wait that follows `delay`: twice as long, up to the ceiling.
    #[must_use]
    pub fn next(&self, delay: Duration) -> Duration {
        delay.saturating_mul(2).min(self.max)
    }

    /// Returns the wait after the failed attempt numbered `attempts`,
    /// counting from one: `initial·2^(attempts-1)`, capped at `max`.
    #[must_use]
    pub fn after(&self, attempts: u32) -> Duration {
        2_u32
            .checked_pow(attempts.saturating_sub(1))
            .and_then(|factor| self.initial.checked_mul(factor))
            .unwrap_or(self.max)
            .min(self.max)
    }

    /// Returns the wait after the failed attempt numbered `attempts`, of
    /// which `random`, drawn uniformly over `u32`, keeps between half and
    /// all.
    #[must_use]
    pub fn jittered(&self, attempts: u32, random: u32) -> Duration {
        let delay = self.after(attempts).as_nanos();
        let half = delay >> 1;
        let jitter = half.saturating_mul(u128::from(random)) >> 32;
        let nanos = delay.saturating_sub(half).saturating_add(jitter);
        Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }
}

/// The waits of one loop: each failure waits its [`Backoff`] and doubles the
/// next, and a success starts again from `initial`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Waits {
    /// The backoff the waits follow.
    backoff: Backoff,
    /// The wait the next failure takes.
    delay: Duration,
}

impl Waits {
    /// Returns the waits of `backoff`, the first at `initial`.
    #[must_use]
    pub const fn new(backoff: Backoff) -> Self {
        Self {
            backoff,
            delay: backoff.initial,
        }
    }

    /// Returns the wait after a failure and doubles the next one.
    pub fn failed(&mut self) -> Duration {
        let wait = self.delay;
        self.delay = self.backoff.next(wait);
        wait
    }

    /// Starts the waits again from `initial` after a success.
    pub const fn reset(&mut self) {
        self.delay = self.backoff.initial;
    }
}

/// Whether a failed attempt may succeed when it is made again unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// A failure of something the work depends on (an unreachable service, a
    /// timeout): a repeat may succeed.
    Transient,
    /// A failure of the work itself (content no mapping admits, a refusal of
    /// the far side): a repeat fails the same way.
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

/// How one lane reads a failure from an HTTP answer's status: every `5xx`
/// (RFC 9110 §15.6) is transient, a `4xx` is poisoned unless the lane lists
/// it in `transient`, and any other status is `other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Statuses {
    /// The client errors the lane counts transient.
    pub transient: &'static [StatusCode],
    /// The failure of a status that is neither a client nor a server error.
    pub other: Failure,
}

impl Statuses {
    /// Returns the failure an answer with `status` means to the lane.
    #[must_use]
    pub fn failure(&self, status: StatusCode) -> Failure {
        if status.is_server_error() || self.transient.contains(&status) {
            Failure::Transient
        } else if status.is_client_error() {
            Failure::Poisoned
        } else {
            self.other
        }
    }
}

/// What happens to work after a failed attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Run again after the delay.
    Retry(Duration),
    /// Stop and wait for an operator.
    Park,
}

/// A retry policy that parks a poisoned failure at once and a transient one
/// after `max_attempts`, waiting a jittered [`Backoff`] in between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// The wait between two attempts.
    pub backoff: Backoff,
    /// The most attempts before the work parks.
    pub max_attempts: u32,
}

impl Policy {
    /// Returns what follows the failed attempt numbered `attempts`, counting
    /// from one, with `random` drawn uniformly over `u32` for the jitter.
    #[must_use]
    pub fn next(&self, failure: Failure, attempts: u32, random: u32) -> Next {
        if failure == Failure::Poisoned || attempts >= self.max_attempts {
            return Next::Park;
        }
        Next::Retry(self.backoff.jittered(attempts, random))
    }
}

/// How an operator settles parked work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Run it again.
    Retry,
    /// Settle it without running it again.
    Discard,
}

impl Decision {
    /// Returns the decision as the stores and the operator API write it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Retry => "retry",
            Self::Discard => "discard",
        }
    }

    /// Returns the decision `text` names, `None` for any other text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "retry" => Some(Self::Retry),
            "discard" => Some(Self::Discard),
            _ => None,
        }
    }
}

impl fmt::Display for Decision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<ferrobridge_operator_api::events::Decision> for Decision {
    fn from(decision: ferrobridge_operator_api::events::Decision) -> Self {
        match decision {
            ferrobridge_operator_api::events::Decision::Retry => Self::Retry,
            ferrobridge_operator_api::events::Decision::Discard => Self::Discard,
        }
    }
}

impl From<Decision> for ferrobridge_operator_api::events::Decision {
    fn from(decision: Decision) -> Self {
        match decision {
            Decision::Retry => Self::Retry,
            Decision::Discard => Self::Discard,
        }
    }
}

/// An operator's settlement of parked work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decided {
    /// Retry or discard.
    pub decision: Decision,
    /// Who decided: the subject of the operator's token.
    pub by: String,
    /// Why, in the operator's words.
    pub reason: String,
}

/// Returns a random `u32` for [`Policy::next`].
///
/// # Errors
/// Returns [`aws_lc_rs::error::Unspecified`] when the system random source
/// fails.
pub fn random() -> Result<u32, aws_lc_rs::error::Unspecified> {
    let mut bytes = [0_u8; 4];
    aws_lc_rs::rand::fill(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Backoff, Waits};

    #[test]
    fn the_backoff_doubles_up_to_its_ceiling() {
        let backoff = Backoff {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(5),
        };
        assert_eq!(Duration::from_secs(2), backoff.next(Duration::from_secs(1)));
        assert_eq!(Duration::from_secs(5), backoff.next(Duration::from_secs(4)));
        assert_eq!(Duration::from_secs(5), backoff.next(Duration::from_secs(5)));
    }

    #[test]
    fn the_waits_double_and_start_again_after_a_success() {
        let mut waits = Waits::new(Backoff {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(3),
        });
        assert_eq!(Duration::from_secs(1), waits.failed());
        assert_eq!(Duration::from_secs(2), waits.failed());
        assert_eq!(Duration::from_secs(3), waits.failed());
        waits.reset();
        assert_eq!(Duration::from_secs(1), waits.failed());
    }
}
