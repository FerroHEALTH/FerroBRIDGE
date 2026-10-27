// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The health document of the bridge's own: `GET /health/info`, the build
//! facts, pins and lanes. `GET /health/readiness` answers the family's
//! readiness document, `ferrohealth_health::document::Readiness`.

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

