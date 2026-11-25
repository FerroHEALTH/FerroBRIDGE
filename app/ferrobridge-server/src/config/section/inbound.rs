// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[inbound]` section: the intake every inbound source shares.
//!
//! No specification governs an inbound intake: our own design.

use ferrohealth_config::units::Duration;
use serde::{Deserialize, Serialize};

/// The inbound intake.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Inbound {
    /// Whether `/inbound/{source}` is served and the workers run.
    pub enabled: bool,
    /// The largest body any source may send, in bytes; a source's own limit
    /// lies within it.
    pub body_limit: usize,
    /// The workers on each replica.
    pub workers: u32,
    /// How long a claimed item and a slot stay held before another worker
    /// may take them.
    pub lease: Duration,
    /// The delay before the first repeat of a failed item.
    pub backoff: Duration,
    /// The longest delay between repeats.
    pub max_backoff: Duration,
    /// The attempts before a failing item parks.
    pub max_attempts: u32,
    /// How long an idle worker waits for a notification before it looks for
    /// due work again.
    pub idle: Duration,
}

impl Default for Inbound {
    fn default() -> Self {
        Self {
            enabled: false,
            body_limit: 1024 * 1024,
            workers: 4,
            lease: Duration::from_secs(60),
            backoff: Duration::from_secs(2),
            max_backoff: Duration::from_secs(600),
            max_attempts: 10,
            idle: Duration::from_secs(5),
        }
    }
}
