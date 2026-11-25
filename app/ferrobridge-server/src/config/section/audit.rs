// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[audit]` section: the Audit Record Repository the access records go
//! to.
//!
//! The records wait in the state store's outbox, and the category map they
//! are classified by is the `audit-categories` settings register.
//!
//! The record is Regulation (EU) 2025/327 Annex II 3.2's, sent as an IHE BALP
//! `AuditEvent` over the ITI-20 ATX: FHIR Feed. The section's shape is our
//! own design; no specification governs the configuration.

use ferrohealth_config::units::Duration;
use ferrohealth_smart::config::endpoint::EndpointSection;
use serde::{Deserialize, Serialize};

/// The access-record lane.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Audit {
    /// Whether the facade and the HL7 v2 face record their accesses.
    pub enabled: bool,
    /// The Audit Record Repository: its FHIR base as `url`, under which
    /// records are created as `[base]/AuditEvent`, the timeouts, the retry
    /// between deliveries, the TLS and the credentials.
    pub endpoint: EndpointSection,
    /// Whether an `http` base is accepted, for a TLS terminator beside the
    /// bridge that the operator answers for.
    pub cleartext: bool,
    /// How the bridge names itself as the recording system
    /// (`AuditEvent.source.observer`); empty takes `[facade] system_id`.
    pub source_id: String,
    /// The site the bridge runs at (`AuditEvent.source.site`).
    pub site: Option<String>,
}

impl Default for Audit {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: EndpointSection {
                retry: ferrohealth_smart::config::endpoint::RetrySection {
                    max_backoff: Duration::from_secs(60),
                    ..ferrohealth_smart::config::endpoint::RetrySection::default()
                },
                ..EndpointSection::default()
            },
            cleartext: false,
            source_id: String::new(),
            site: None,
        }
    }
}
