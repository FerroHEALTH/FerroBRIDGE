// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[cdm]` section.
//!
//! No specification governs the configuration: our own design.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
