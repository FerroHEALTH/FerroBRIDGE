// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[audit]` section: the Audit Record Repository the access records go
//! to, the spool they wait in, and the category map they are classified by.
//!
//! The record is Regulation (EU) 2025/327 Annex II 3.2's, sent as an IHE BALP
//! `AuditEvent` over the ITI-20 ATX: FHIR Feed. The section's shape is our
//! own design; no specification governs the configuration.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ehds_logging::map::Declared;
use ferrohealth_config::units::{ByteSize, Duration};
use ferrohealth_smart::config::credentials::CredentialsSection;
use serde::{Deserialize, Serialize};

/// The access-record lane.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Audit {
    /// Whether the facade and the HL7 v2 face record their accesses.
    pub enabled: bool,
    /// The FHIR base of the Audit Record Repository, which records are
    /// created under as `[base]/AuditEvent`.
    pub base_url: String,
    /// Whether an `http` base is accepted, for a TLS terminator beside the
    /// bridge that the operator answers for.
    pub cleartext: bool,
    /// How long one delivery to the repository may take.
    pub timeout: Duration,
    /// The longest wait between two delivery attempts.
    pub retry_max: Duration,
    /// The directory records are stored in until the repository takes them.
    pub spool_dir: Option<PathBuf>,
    /// The most records the spool holds at once.
    pub max_records: usize,
    /// The most bytes the spool holds at once.
    pub max_size: ByteSize,
    /// The longest storing one record may take.
    pub write_timeout: Duration,
    /// How the bridge names itself as the recording system
    /// (`AuditEvent.source.observer`); empty takes `[facade] system_id`.
    pub source_id: String,
    /// The site the bridge runs at (`AuditEvent.source.site`).
    pub site: Option<String>,
    /// The credentials the repository expects.
    pub credentials: CredentialsSection,
    /// The categories the data under each template and archetype belong to.
    pub categories: AuditCategories,
}

impl Default for Audit {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            cleartext: false,
            timeout: Duration::from_secs(10),
            retry_max: Duration::from_secs(60),
            spool_dir: None,
            max_records: 10_000,
            max_size: ByteSize::from_mib(64),
            write_timeout: Duration::from_secs(2),
            source_id: String::new(),
            site: None,
            credentials: CredentialsSection::default(),
            categories: AuditCategories::default(),
        }
    }
}

/// The deployment's category map (Annex II 3.2(c)): each template id and
/// archetype id to its Art 14(1) categories, or to `none`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuditCategories {
    /// The national categories the map may name, each `<system>|<code>`.
    pub national: Vec<String>,
    /// The categories of the data under each template id.
    pub templates: BTreeMap<String, Declared>,
    /// The categories of the data under each archetype id.
    pub archetypes: BTreeMap<String, Declared>,
}
