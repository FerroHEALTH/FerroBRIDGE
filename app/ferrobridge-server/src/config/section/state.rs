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
    /// The most entries the replica's copy of each record-once identity
    /// table holds; `0` keeps none.
    pub identity_copy_entries: usize,
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
            // NOTE: PostgreSQL 19 docs, "Client Connection Defaults": a state statement is one short
            // read or write, a held lock waits no longer than a connection does, and an abandoned
            // transaction releases its locks within a minute.
            statement_timeout: Duration::from_secs(30),
            lock_timeout: Duration::from_secs(5),
            idle_in_transaction_session_timeout: Duration::from_secs(60),
            statement_cache_capacity: 100,
            max_lifetime: Duration::from_secs(30 * 60),
            idle_timeout: Duration::from_secs(10 * 60),
            identity_copy_entries: 10_000,
            seal: None,
        }
    }
}

/// The sealing keys, the first sealing and every one opening what it
/// sealed, and the lookup key identifier values are digested under.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct StateSeal {
    /// The keys, newest first.
    pub keys: Vec<SealKey>,
    /// The base64 of the lookup key's 32 bytes; it never rotates.
    pub lookup_key: Option<Secret>,
    /// A file holding that base64, read at boot.
    pub lookup_key_file: Option<PathBuf>,
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
