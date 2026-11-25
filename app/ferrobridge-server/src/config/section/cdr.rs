// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[cdr]` section.
//!
//! No specification governs the configuration: our own design.

use ferrohealth_config::units::Duration;
use serde::{Deserialize, Serialize};

use ferrohealth_smart::config::endpoint::EndpointSection;
use ferrohealth_smart::config::onward::OnwardSection;

/// The openEHR CDR lane.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cdr {
    /// The CDR: the openEHR REST API root as `url`, the path `/ehr` and
    /// `/query` live under, the timeouts, the retry budget of idempotent
    /// calls, the TLS and the credentials.
    pub endpoint: EndpointSection,
    /// Whether the bridge reaches the ITS-REST 1.1.0 Demographic API of the
    /// CDR, a module the release marks `x-status: DEVELOPMENT`.
    pub demographic: bool,
    /// The onward issuers beside `endpoint.credentials.onward`, one per kind,
    /// each authenticated with `endpoint.credentials.oauth`.
    pub onward: Vec<OnwardSection>,
    /// The issuer the CDR names a Basic user of `endpoint.credentials` under,
    /// so a CONTRIBUTION states the committer placeholder that CDR accepts: a
    /// `PARTY_IDENTIFIED` whose one `DV_IDENTIFIER` is this issuer and the
    /// user.
    pub committer_issuer: Option<String>,
}

impl Default for Cdr {
    fn default() -> Self {
        Self {
            endpoint: EndpointSection {
                timeout: Duration::from_secs(30),
                ..EndpointSection::default()
            },
            demographic: false,
            onward: Vec::new(),
            committer_issuer: None,
        }
    }
}
