// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's configuration: a TOML file, `FERROBRIDGE_VIEWER__`
//! environment overrides over it, and every secret reachable through a
//! `<key>_file` sibling read at start.
//!
//! [`Config`] is the tree as written, every default in its struct's
//! `Default` impl; [`Config::load`](crate::config::Config::load) reads it and
//! [`Config::resolve`](crate::config::Config::resolve) turns it into the
//! checked [`settings::Settings`] the server runs on. An override names its
//! key by path, `FERROBRIDGE_VIEWER__BRIDGE__BASE_URL` for `[bridge]
//! base_url`. No specification governs the configuration: our own design.
//!
//! ```toml
//! [server]
//! listen = "0.0.0.0:3000"
//! site_root = "/site"
//!
//! [bridge]
//! base_url = "https://bridge.example.org/"
//! timeout_ms = 30000
//! ```

pub mod error;
pub mod load;
pub mod secret;
pub mod settings;

use std::path::PathBuf;

use serde::Deserialize;

/// The environment variable that names the configuration file when
/// `--config` does not.
pub const CONFIG_PATH_ENV: &str = "FERROBRIDGE_VIEWER_CONFIG";

/// The prefix of every environment override.
pub const ENV_PREFIX: &str = "FERROBRIDGE_VIEWER__";

/// The configuration tree as written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// `[server]`: where the console listens and where its site bundle is.
    pub server: Server,
    /// `[bridge]`: the FerroBRIDGE server the console is a client of.
    pub bridge: Bridge,
}

/// `[server]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    /// The socket address the console listens on.
    pub listen: String,
    /// The directory holding the site bundle cargo-leptos builds: the
    /// WebAssembly, its JavaScript glue and the stylesheet under `pkg/`.
    pub site_root: PathBuf,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: String::from("127.0.0.1:3000"),
            site_root: PathBuf::from("target/site"),
        }
    }
}

/// `[bridge]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Bridge {
    /// The root of the bridge's HTTP surface, under which it serves `/fhir`
    /// and `/health`.
    pub base_url: String,
    /// How long one call to the bridge may take, in milliseconds.
    pub timeout_ms: u64,
}

impl Default for Bridge {
    fn default() -> Self {
        Self {
            base_url: String::from("http://127.0.0.1:8080/"),
            timeout_ms: 30_000,
        }
    }
}
