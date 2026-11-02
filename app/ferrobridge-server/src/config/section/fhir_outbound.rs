// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[fhir_outbound]` section: the lane that follows the change feed and
//! delivers each changed composition as FHIR R4 to a configured endpoint.
//!
//! No specification governs the outbound lane: our own design. It is off
//! until `enabled` is set, and it follows the `[change_feed]` source.

use std::path::PathBuf;

use ferrohealth_smart::config::CredentialsSection;
use serde::{Deserialize, Serialize};

/// The outbound FHIR lane.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FhirOutbound {
    /// Whether the lane follows the change feed.
    pub enabled: bool,
    /// The reader name the lane follows the change feed under.
    pub reader: String,
    /// The FHIR service base each transaction Bundle is posted to.
    pub base_url: String,
    /// Whether an `http` base is accepted, for a TLS terminator beside the
    /// bridge that the operator answers for.
    pub cleartext: bool,
    /// How long one delivery may take.
    pub timeout_ms: u64,
    /// The file the record of what was delivered is kept in.
    pub emitted: Option<PathBuf>,
    /// The credentials the endpoint expects.
    pub credentials: CredentialsSection,
}

impl Default for FhirOutbound {
    fn default() -> Self {
        Self {
            enabled: false,
            reader: String::new(),
            base_url: String::new(),
            cleartext: false,
            timeout_ms: 30_000,
            emitted: None,
            credentials: CredentialsSection::default(),
        }
    }
}
