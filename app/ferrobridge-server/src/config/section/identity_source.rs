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
    /// The replica's cache of the pseudonyms the identity source answered,
    /// kept true by its PMIR feed; absent, every lookup asks the source.
    pub cache: Option<PseudonymCache>,
}

/// The `[identity_source.cache]` table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PseudonymCache {
    /// The most pseudonyms the replica keeps.
    pub entries: usize,
    /// The longest one pseudonym is kept, whatever the feed says.
    pub max_age: Duration,
    /// This bridge's PMIR feed receiver as the identity source reaches it,
    /// the subscription's channel endpoint.
    pub feed_url: String,
    /// The client ids of the access tokens the receiver admits.
    pub feed_clients: Vec<String>,
    /// How often the subscription's holder reads its status.
    pub status_every: Duration,
}

impl Default for PseudonymCache {
    fn default() -> Self {
        Self {
            entries: 10_000,
            max_age: Duration::from_secs(5 * 60),
            feed_url: String::new(),
            feed_clients: Vec::new(),
            status_every: Duration::from_secs(30),
        }
    }
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
            cache: None,
        }
    }
}
