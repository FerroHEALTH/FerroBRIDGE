// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[change_feed]` section: the source the change-feed consumers follow.
//!
//! No specification governs the change feed: our own design. The section is
//! off until `enabled` is set, and a consumer section names the reader it
//! follows the source under.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The change-feed source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChangeFeed {
    /// Whether the consumers follow the source.
    pub enabled: bool,
    /// The source: `aql`, the vendor-neutral poll of `/query/aql` that works
    /// against any CDR, or `ferroehr`, the reference CDR's outbox feed.
    pub source: String,
    /// The root of the reference CDR's extensions, the path its
    /// `/admin/outbox` routes live under; `ferroehr` only.
    pub url: String,
    /// The `redb` file the `aql` source keeps each reader's watermark in.
    pub watermarks: Option<PathBuf>,
    /// The `redb` file the consumers park a poisoned event in, and count each
    /// event's poisoned passes in.
    pub dead_letters: Option<PathBuf>,
    /// The poisoned passes an event fails before it is parked, 1 to 1000.
    pub max_attempts: u64,
    /// Where an `aql` reader with no watermark starts: `now`, `beginning`, or
    /// an RFC 3339 instant.
    pub start: String,
    /// How many events one read asks for, 1 to 1000.
    pub page_size: u64,
    /// How long to wait after a read that found no full page.
    pub poll_interval_ms: u64,
    /// How long to wait after the first failed pass, doubled per failure.
    pub backoff_ms: u64,
    /// The longest wait after a failed pass.
    pub max_backoff_ms: u64,
}

impl Default for ChangeFeed {
    fn default() -> Self {
        Self {
            enabled: false,
            source: String::from("aql"),
            url: String::new(),
            watermarks: None,
            dead_letters: None,
            max_attempts: 5,
            start: String::from("now"),
            page_size: 100,
            poll_interval_ms: 1_000,
            backoff_ms: 1_000,
            max_backoff_ms: 60_000,
        }
    }
}
