// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[events]` section: the AMQP publisher of the commit events the change
//! feed reads.
//!
//! No specification governs the lane: our own design. It is off until
//! `enabled` is set, and it follows the `[change_feed]` source.

use ferrohealth_config::units::Duration;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use ferrohealth_secret::secret::Secret;

/// The events lane.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Events {
    /// Whether the lane publishes.
    pub enabled: bool,
    /// The reader name the lane follows the change feed under.
    pub reader: String,
    /// The AMQP 0.9.1 broker URL, which may carry credentials.
    pub url: Option<Secret>,
    /// A file holding the broker URL, read once at boot.
    pub url_file: Option<PathBuf>,
    /// Whether an `amqp://` URL is upgraded to `amqps://`.
    pub tls: bool,
    /// The durable topic exchange the events are published to.
    pub exchange: String,
    /// How long one publish may wait for the broker's confirm.
    pub confirm_timeout: Duration,
    /// The durable queues declared and bound to the exchange.
    pub queues: Vec<EventsQueue>,
}

impl Default for Events {
    fn default() -> Self {
        Self {
            enabled: false,
            reader: String::new(),
            url: None,
            url_file: None,
            tls: false,
            exchange: String::new(),
            confirm_timeout: Duration::from_secs(30),
            queues: Vec::new(),
        }
    }
}

/// One durable queue bound to the events exchange.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct EventsQueue {
    /// The queue name.
    pub name: String,
    /// The topic binding key, such as `COMPOSITION.*.*` or `#`.
    pub binding: String,
}
