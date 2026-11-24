// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[state]` section: the bridge's own PostgreSQL database.
//!
//! Every replica keeps its state there, and seals stored bodies under the
//! keys of `[state.seal]`. No specification governs where the bridge keeps
//! its state: our own design.

use std::path::PathBuf;

use ferrohealth_config::units::Duration;
use ferrohealth_secret::secret::Secret;
use serde::{Deserialize, Serialize};

/// The state store.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct State {
    /// The PostgreSQL connection URL; it must name its `sslmode`.
    pub url: Option<Secret>,
    /// A file holding the connection URL, read at boot.
    pub url_file: Option<PathBuf>,
    /// The PEM CA the database's certificate is checked against.
    pub tls_ca: Option<String>,
    /// A file holding that CA, read at boot.
    pub tls_ca_file: Option<PathBuf>,
    /// The most connections the pool holds.
    pub max_connections: u32,
    /// How long a request waits for a connection before the store answers
    /// that it is unavailable.
    pub acquire_timeout: Duration,
    /// The keys stored bodies are sealed under, when any area seals.
    pub seal: Option<StateSeal>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            url: None,
            url_file: None,
            tls_ca: None,
            tls_ca_file: None,
            max_connections: 10,
            acquire_timeout: Duration::from_secs(5),
            seal: None,
        }
    }
}

/// The sealing keys: the first seals, every one opens what it sealed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct StateSeal {
    /// The keys, newest first.
    pub keys: Vec<SealKey>,
}

/// One sealing key and the id a sealed row names it by.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SealKey {
    /// The id, 1 to 64 characters of `A-Z`, `a-z`, `0-9`, `.`, `_` and `-`.
    pub id: String,
    /// The base64 of the key's 32 bytes.
    pub key: Option<Secret>,
    /// A file holding that base64, read at boot.
    pub key_file: Option<PathBuf>,
}
