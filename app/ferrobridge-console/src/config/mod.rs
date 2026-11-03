// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's configuration, read by `ferrohealth-config`.
//!
//! The tree's defaults, a TOML file, `FERROBRIDGE_CONSOLE__` environment overrides and
//! the `--set` pairs over it, and every secret reachable through a
//! `<key>_file` sibling read at start through `ferrohealth-secret`.
//!
//! [`Config`] is the tree as written, every default in its struct's
//! `Default` impl; [`Config::load`](crate::config::Config::load) reads it and
//! [`Config::resolve`](crate::config::Config::resolve) turns it into the
//! checked [`settings::Settings`] the server runs on. An override names its
//! key by path, `FERROBRIDGE_CONSOLE__BRIDGE__BASE_URL` for `[bridge]
//! base_url`; `RUST_LOG` is the alias of `telemetry.filter`. No
//! specification governs the configuration: our own design.
//!
//! ```toml
//! [server]
//! listen = "0.0.0.0:3000"
//! locale = "en"
//!
//! [bridge]
//! base_url = "https://bridge.example.org/"
//! timeout_ms = 30000
//!
//! [oidc]
//! issuer = "https://idp.example.org/realms/ferrobridge"
//! authorization_endpoint = "https://idp.example.org/realms/ferrobridge/protocol/openid-connect/auth"
//! token_endpoint = "https://idp.example.org/realms/ferrobridge/protocol/openid-connect/token"
//! jwks_uri = "https://idp.example.org/realms/ferrobridge/protocol/openid-connect/certs"
//! client_id = "ferrobridge-console"
//! client_secret_file = "/run/secrets/console-client-secret"
//! redirect_uri = "https://console.example.org/auth/callback"
//! end_session_endpoint = "https://idp.example.org/realms/ferrobridge/protocol/openid-connect/logout"
//! post_logout_redirect_uri = "https://console.example.org/"
//! offline_access = true
//! ```
//!
//! `[oidc]` and `[session]` are the sections of `ferrohealth-session`
//! (`ferrohealth_session::config`): with `issuer` alone the provider's
//! endpoints are discovered when the console starts, each endpoint written
//! takes the place of the discovered one, and `[session] store = "sealed"`
//! with a `key` or `key_file` keeps the session in an encrypted cookie in
//! place of the console's memory. `config check` contacts no provider.

pub mod error;
pub mod load;
pub mod settings;

use ferrohealth_config::loader::Loader;
use ferrohealth_session::config::{OidcSection, SessionSection};
use ferrohealth_telemetry::section::TelemetrySection;
use serde::{Deserialize, Serialize};

/// The environment namespace: `FERROBRIDGE_CONSOLE__SERVER__LISTEN` sets
/// `[server] listen`, and `FERROBRIDGE_CONSOLE_CONFIG` names the file.
pub const PREFIX: &str = "FERROBRIDGE_CONSOLE";

/// The environment variable that names the configuration file when
/// `--config` does not.
pub const CONFIG_PATH_ENV: &str = "FERROBRIDGE_CONSOLE_CONFIG";

/// The prefix of every environment override.
pub const ENV_PREFIX: &str = "FERROBRIDGE_CONSOLE__";

/// The variable that names the `tracing` filter, the alias of
/// `telemetry.filter`.
pub const FILTER_ENV: &str = "RUST_LOG";

/// The loader of the console's tree.
///
/// # Errors
/// Returns the loader's error when the prefix is refused, which it never is
/// for [`PREFIX`].
pub fn loader() -> Result<Loader, ferrohealth_config::error::ConfigError> {
    Ok(Loader::new(PREFIX)?.alias(FILTER_ENV, "telemetry.filter"))
}

/// The configuration tree as written.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// `[server]`: where the console listens and the locale it falls back to.
    pub server: Server,
    /// `[bridge]`: the FerroBRIDGE server the console is a client of.
    pub bridge: Bridge,
    /// `[session]`: the store and the lifetimes of the sign-in sessions.
    pub session: SessionSection,
    /// `[oidc]`: the OpenID Provider operators sign in with; without it the
    /// console offers no sign-in.
    pub oidc: Option<OidcSection>,
    /// `[telemetry]`: the console log, the family's section; `RUST_LOG` sets
    /// its filter too.
    pub telemetry: TelemetrySection,
}

/// `[server]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    /// The socket address the console listens on.
    pub listen: String,
    /// The BCP 47 tag of the locale a page renders in when the request's
    /// `Accept-Language` names no language the console's catalog holds.
    pub locale: String,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: String::from("127.0.0.1:3000"),
            locale: String::from("en"),
        }
    }
}

/// `[bridge]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
