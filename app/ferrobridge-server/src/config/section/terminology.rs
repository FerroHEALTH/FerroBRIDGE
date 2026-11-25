// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[terminology]` section.
//!
//! No specification governs the configuration: our own design.

use ferrohealth_config::units::Duration;
use serde::{Deserialize, Serialize};

use ferrohealth_smart::config::endpoint::EndpointSection;

/// The FHIR terminology lane.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Terminology {
    /// The terminology server: its FHIR service base as `url`, the timeouts,
    /// the retry budget, the TLS and the credentials.
    pub endpoint: EndpointSection,
    /// The release the server answers in: `stu3`, `r4` or `r4b`.
    pub wire_version: String,
    /// Whether a code the server does not know takes `coding.code` as its
    /// display going into openEHR, rather than refusing the unit.
    pub display_fallback: bool,
}

impl Default for Terminology {
    fn default() -> Self {
        Self {
            endpoint: EndpointSection {
                timeout: Duration::from_secs(30),
                ..EndpointSection::default()
            },
            wire_version: String::from("r4"),
            display_fallback: false,
        }
    }
}
