// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[shl]` section: the person's SMART Health Links.
//!
//! The limits a link is held to are SMART Health Cards and Links 1.0.0's
//! where it sets one (a `location` lives at most one hour); the rest is our
//! own design.

use std::path::PathBuf;

use ferrohealth_config::units::Duration;
use serde::{Deserialize, Serialize};

/// The links lane.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Shl {
    /// Whether the person's links are served.
    pub enabled: bool,
    /// The `redb` file the links are kept in.
    pub store: Option<PathBuf>,
    /// The public origin a receiver reaches the bridge at, such as
    /// `https://bridge.example.org`; a link's URL is this origin, `/shl/m/`
    /// and 43 characters, at most 128 in all.
    pub base_url: String,
    /// How long a link lives when its creator names no expiry.
    pub default_expiry: Duration,
    /// The longest a link may live.
    pub max_expiry: Duration,
    /// The most answers a link may give.
    pub max_answers: u32,
    /// The wrong passcodes a link takes before it is disabled.
    pub passcode_attempts: u32,
    /// How long a file location lives, at most one hour.
    pub location_lifetime: Duration,
}

impl Default for Shl {
    fn default() -> Self {
        Self {
            enabled: false,
            store: None,
            base_url: String::new(),
            default_expiry: Duration::from_secs(24 * 3600),
            max_expiry: Duration::from_secs(7 * 24 * 3600),
            max_answers: 10,
            passcode_attempts: 5,
            location_lifetime: Duration::from_secs(3600),
        }
    }
}
