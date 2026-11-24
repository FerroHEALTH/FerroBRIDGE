// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! When failed inbound work runs again, and when it stops and waits for an
//! operator.
//!
//! A transient failure (the network, a `5xx`, `429` or `423`) runs again
//! after an exponential backoff with jitter until the attempts run out; a
//! refusal parks at once. No specification governs an intake's retry policy:
//! our own design.

use std::time::Duration;

use http::StatusCode;

/// How a remote call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The call may succeed when made again.
    Transient,
    /// The far side refused the content, so a repeat fails the same way.
    Refusal,
}

impl Failure {
    /// Returns how an answer with `status` failed.
    ///
    /// A `5xx` (RFC 9110 §15.6), `429 Too Many Requests` (RFC 6585 §4) and
    /// `423 Locked` (RFC 4918 §11.3) are transient; every other failure
    /// status is a refusal.
    #[must_use]
    pub fn of(status: StatusCode) -> Self {
        if status.is_server_error()
            || status == StatusCode::TOO_MANY_REQUESTS
            || status == StatusCode::LOCKED
        {
            Self::Transient
        } else {
            Self::Refusal
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

/// The retry policy of one source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// The delay before the first repeat.
    pub backoff: Duration,
    /// The longest delay.
    pub max_backoff: Duration,
    /// The most attempts before the work parks.
    pub max_attempts: u32,
}

impl Policy {
    /// Returns what follows the failed attempt numbered `attempts`, counting
    /// from one, with `random` drawn uniformly over `u32`.
    ///
    /// The delay is `backoff·2^(attempts-1)`, capped at `max_backoff`, of
    /// which `random` keeps between half and all.
    #[must_use]
    pub fn next(&self, failure: Failure, attempts: u32, random: u32) -> Next {
        if failure == Failure::Refusal || attempts >= self.max_attempts {
            return Next::Park;
        }
        let doubled = 2_u32
            .checked_pow(attempts.saturating_sub(1))
            .and_then(|factor| self.backoff.checked_mul(factor))
            .unwrap_or(self.max_backoff);
        let delay = doubled.min(self.max_backoff).as_nanos();
        let half = delay >> 1;
        let jitter = half.saturating_mul(u128::from(random)) >> 32;
        let nanos = delay.saturating_sub(half).saturating_add(jitter);
        Next::Retry(Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX)))
    }
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
