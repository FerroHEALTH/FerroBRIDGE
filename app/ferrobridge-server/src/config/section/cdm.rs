// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[cdm]` section.
//!
//! No specification governs the configuration: our own design.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use ferrohealth_config::units::Duration;
use ferrohealth_secret::secret::Secret;

/// The OMOP CDM database lane.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cdm {
    /// The PostgreSQL connection URL.
    pub url: Option<Secret>,
    /// A file holding the connection URL, read at boot.
    pub url_file: Option<PathBuf>,
    /// The PEM CA the database's certificate is checked against, for the
    /// URL's `sslmode` of `require`, `verify-ca` or `verify-full`.
    pub tls_ca: Option<String>,
    /// A file holding that CA, read at boot.
    pub tls_ca_file: Option<PathBuf>,
    /// The schema the CDM tables live in.
    pub schema: String,
    /// The schema the bridge keeps its natural-key side table in.
    pub bridge_schema: String,
    /// What the writer does for an EHR with no `PERSON` row:
    /// `create_on_first_sight` (a placeholder row) or `existing` (a refusal).
    pub person_policy: String,
    /// The longest one statement runs before the server cancels it; `0s`
    /// disables it.
    pub statement_timeout: Duration,
    /// The longest a statement waits for a lock; `0s` disables it.
    pub lock_timeout: Duration,
    /// The longest a session stays idle inside an open transaction; `0s`
    /// disables it.
    pub idle_in_transaction_session_timeout: Duration,
    /// How many prepared statements each pooled connection keeps.
    pub statement_cache_capacity: usize,
    /// The longest a pooled connection lives.
    pub max_lifetime: Duration,
    /// The longest a pooled connection stays idle.
    pub idle_timeout: Duration,
    /// The pseudonymisation of every row the writer writes, off until the
    /// section is present.
    pub pseudonymisation: Option<CdmPseudonymisation>,
}

impl Default for Cdm {
    fn default() -> Self {
        Self {
            url: None,
            url_file: None,
            tls_ca: None,
            tls_ca_file: None,
            schema: String::from("cdm"),
            bridge_schema: String::from("ferrobridge"),
            person_policy: String::from("create_on_first_sight"),
            // NOTE: PostgreSQL 19 docs, "Client Connection Defaults": a vocabulary load or a derived
            // table build runs one statement for minutes, so no statement timeout; a lock or an open
            // transaction still ends.
            statement_timeout: Duration::from_secs(0),
            lock_timeout: Duration::from_secs(60),
            idle_in_transaction_session_timeout: Duration::from_secs(10 * 60),
            statement_cache_capacity: 100,
            max_lifetime: Duration::from_secs(30 * 60),
            idle_timeout: Duration::from_secs(10 * 60),
            pseudonymisation: None,
        }
    }
}

/// The pseudonymisation of the rows the CDM writer writes: a keyed pseudonym
/// for every source identifier and an optional per-person date shift.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CdmPseudonymisation {
    /// The HMAC key, at least 32 bytes.
    pub key: Option<Secret>,
    /// A file holding the key, read at boot.
    pub key_file: Option<PathBuf>,
    /// The largest shift of a person's dates either way, in days; 0 leaves
    /// dates as they are.
    pub date_shift_days: u16,
}
