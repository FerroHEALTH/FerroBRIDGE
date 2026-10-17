// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The checked settings the console runs on, resolved from the written
//! [`Config`].

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use url::Url;

use crate::config::Config;
use crate::config::error::Error;

/// The resolved configuration.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The socket address the console listens on.
    pub listen: SocketAddr,
    /// The directory holding the site bundle.
    pub site_root: PathBuf,
    /// The bridge the console is a client of.
    pub bridge: BridgeSettings,
}

/// The resolved `[bridge]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeSettings {
    /// The root of the bridge's HTTP surface, an `http` or `https` URL whose
    /// path ends in `/`, so a relative route joins beneath it.
    pub base: Url,
    /// How long one call to the bridge may take.
    pub timeout: Duration,
}

impl Config {
    /// Checks every value.
    ///
    /// # Errors
    /// Returns [`Error::Address`] for a listen address that does not parse,
    /// [`Error::Missing`], [`Error::Url`] and [`Error::UrlShape`] for a
    /// bridge URL that is absent, does not parse or is not one the key
    /// admits, and [`Error::Zero`] for a zero timeout.
    pub fn resolve(&self) -> Result<Settings, Error> {
        let listen = self
            .server
            .listen
            .parse()
            .map_err(|source| Error::Address {
                key: String::from("server.listen"),
                source,
            })?;
        let base = base_url("bridge.base_url", &self.bridge.base_url)?;
        if self.bridge.timeout_ms == 0 {
            return Err(Error::Zero {
                key: String::from("bridge.timeout_ms"),
            });
        }
        Ok(Settings {
            listen,
            site_root: self.server.site_root.clone(),
            bridge: BridgeSettings {
                base,
                timeout: Duration::from_millis(self.bridge.timeout_ms),
            },
        })
    }
}

/// Parses an `http` or `https` URL with a host, no userinfo, no query and no
/// fragment, and ends its path in `/`.
fn base_url(key: &str, text: &str) -> Result<Url, Error> {
    if text.trim().is_empty() {
        return Err(Error::Missing {
            key: key.to_owned(),
        });
    }
    let mut url = Url::parse(text).map_err(|source| Error::Url {
        key: key.to_owned(),
        source,
    })?;
    let shape = |reason| Error::UrlShape {
        key: key.to_owned(),
        reason,
    };
    if !matches!(url.scheme(), "http" | "https") {
        return Err(shape("must be an http or https URL"));
    }
    if url.host().is_none() {
        return Err(shape("must name a host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(shape("must not carry userinfo"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(shape("must not carry a query or a fragment"));
    }
    // NOTE: RFC 3986 §5.2.3: a reference merges with the base path up to its
    // last `/`, so the base ends in one or its last segment would be dropped.
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}
