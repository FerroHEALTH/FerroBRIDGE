// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[identity_source]` section: the `PIXm` Manager every EHR subject is
//! asked of (`PIXm` 3.1.0 ITI-83), which the facade and the inbound engine
//! both read.

use ferrohealth_config::units::Duration;
use ferrohealth_smart::config::endpoint::EndpointSection;
use serde::{Deserialize, Serialize};

/// The identity source.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct IdentitySource {
    /// The `PIXm` Manager: its FHIR base as `url`, the timeouts, the retry,
    /// the TLS and the credentials.
    pub endpoint: EndpointSection,
    /// The identifier system of the pseudonym every EHR subject is, the
    /// ITI-83 `targetSystem`.
    pub pseudonym_domain: String,
    /// The identifier systems a source identifier may be in, in the order a
    /// Patient's identifiers are chosen by.
    pub source_domains: Vec<String>,
}

impl Default for IdentitySource {
    fn default() -> Self {
        Self {
            endpoint: EndpointSection {
                timeout: Duration::from_secs(10),
                ..EndpointSection::default()
            },
            pseudonym_domain: String::new(),
            source_domains: Vec::new(),
        }
    }
}
