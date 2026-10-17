// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The health documents: `GET /health/info`, the build facts, pins and
//! lanes, and `GET /health/readiness`, the state of every indicator.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What `GET /health/info` answers with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Info {
    /// The product name.
    pub product: String,
    /// The product version.
    pub version: String,
    /// The build facts.
    pub build: Build,
    /// The pins the process serves, in the order the banner prints them.
    pub pins: Vec<Pin>,
    /// The lanes this deployment configures, in banner order.
    pub lanes: Vec<Lane>,
}

/// The build facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Build {
    /// The full git commit, or `unknown`.
    pub commit: String,
    /// The abbreviated git commit, or `unknown`.
    pub commit_short: String,
    /// The build instant, RFC 3339 UTC.
    pub built_at: String,
    /// The compiler.
    pub rustc: String,
}

/// One pin the process serves: a name and the version it is at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    /// What is pinned, as the pin matrix names it.
    pub name: String,
    /// The version, as the implementing crate states it.
    pub version: String,
}

/// One lane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lane {
    /// The lane's name, as the banner names it.
    pub name: String,
    /// Whether the configuration switches it on.
    pub enabled: bool,
    /// The socket address a face with a listener of its own binds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listen: Option<String>,
    /// Whether that listener accepts connections now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up: Option<bool>,
}

/// The state of one indicator, or of the aggregate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// The subsystem answered.
    Up,
    /// The subsystem did not answer.
    Down,
}

/// The outcome of one indicator check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndicatorState {
    /// Whether the subsystem answered.
    pub state: State,
    /// Why it did not, when it did not. Never a body and never a credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl IndicatorState {
    /// Returns the state of a subsystem that answered.
    #[must_use]
    pub const fn up() -> Self {
        Self {
            state: State::Up,
            detail: None,
        }
    }

    /// Returns the state of a subsystem that did not answer, with the reason.
    #[must_use]
    pub fn down(detail: impl Into<String>) -> Self {
        Self {
            state: State::Down,
            detail: Some(detail.into()),
        }
    }
}

/// What `GET /health/readiness` answers with, `200` while the aggregate is
/// up and `503` while it is down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Readiness {
    /// The aggregate: down while any indicator is down.
    pub state: State,
    /// Every indicator by name, in name order.
    pub indicators: BTreeMap<String, IndicatorState>,
}
