// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The checked settings the console runs on, resolved from the written
//! [`Config`] with every `_file` secret read.

use std::net::SocketAddr;
use std::time::Duration;

use ferrohealth_session::config::{OidcSection, SessionSettings};
use ferrohealth_ui::i18n::Locale;
use url::Url;

use crate::config::Config;
use crate::config::error::Error;

/// The resolved configuration.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The socket address the console listens on.
    pub listen: SocketAddr,
    /// The locale a page renders in when the request names none the
    /// console's catalog holds.
    pub locale: Locale,
    /// The bridge the console is a client of.
    pub bridge: BridgeSettings,
    /// The store and the lifetimes of the sign-in sessions, as
    /// `ferrohealth-session` checked them.
    pub session: SessionSettings,
    /// The OpenID Provider operators sign in with, when one is configured.
    pub oidc: Option<OidcSettings>,
    /// The console log, the family's `[telemetry]` section.
    pub telemetry: ferrohealth_telemetry::section::TelemetrySection,
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

/// The resolved `[oidc]`: the section `ferrohealth-session` checked, and
/// what the console reads from it for its own pages.
#[derive(Debug, Clone)]
pub struct OidcSettings {
    /// The checked section, which builds the console's relying party.
    pub checked: ferrohealth_session::config::OidcSettings,
    /// The provider's issuer identifier, exactly as written.
    issuer: String,
    /// The console's redirection endpoint, whose origin is the console's.
    redirect_uri: Url,
    /// Whether the section names the authorization, token and key-set
    /// endpoints, so the relying party is built with no discovery.
    endpoints_given: bool,
}

impl OidcSettings {
    /// The provider's issuer identifier, exactly as written: an ID Token's
    /// `iss` must equal it (OpenID Connect Core 1.0 §3.1.3.7).
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// The console's own origin, `scheme://host:port`: the origin of its
    /// redirection endpoint, which an unsafe request must come from.
    #[must_use]
    pub fn origin(&self) -> String {
        self.redirect_uri.origin().ascii_serialization()
    }

    /// The console's redirection endpoint.
    #[must_use]
    pub const fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }

    /// Whether the relying party is built from the written endpoints alone,
    /// with no call to the provider.
    #[must_use]
    pub const fn endpoints_given(&self) -> bool {
        self.endpoints_given
    }
}

impl Config {
    /// Checks every value and reads every `_file` secret, contacting no
    /// provider.
    ///
    /// # Errors
    /// Returns [`Error::Address`] for a listen address that does not parse,
    /// [`Error::Locale`] for a `server.locale` that is no language tag,
    /// [`Error::Missing`], [`Error::Url`] and [`Error::UrlShape`] for a URL
    /// that is absent, does not parse or is not one its key admits,
    /// [`Error::Zero`] for a zero timeout, lifetime or pool bound,
    /// [`Error::Scope`] for `offline_access` listed in `oidc.scopes`,
    /// [`Error::InsecureCookie`] for `session.secure_cookie = false` on a
    /// console that is not `http` on loopback, [`Error::SecretFile`] for the
    /// secrets of `oidc.client_secret` and `session.key`, and
    /// [`Error::Session`] for every other refusal of `[oidc]` and
    /// `[session]`.
    pub fn resolve(&self) -> Result<Settings, Error> {
        let listen = self
            .server
            .listen
            .parse()
            .map_err(|source| Error::Address {
                key: String::from("server.listen"),
                source,
            })?;
        let locale = Locale::parse(&self.server.locale).map_err(|source| Error::Locale {
            key: String::from("server.locale"),
            source,
        })?;
        let base = base_url("bridge.base_url", &self.bridge.base_url)?;
        let timeout = positive("bridge.timeout", self.bridge.timeout)?;
        let oidc = self.oidc.as_ref().map(resolve_oidc).transpose()?;
        let session = match &oidc {
            Some(oidc) => self.session.check(&oidc.checked)?,
            None => self.session.check_with(false)?,
        };
        // NOTE: RFC 6265bis §4.1.2.5: a cookie without `Secure` travels over plain
        // HTTP, so it is admitted only where the console itself is HTTP on loopback.
        if !session.secure_cookie()
            && oidc.as_ref().is_some_and(|oidc| {
                !(oidc.redirect_uri.scheme() == "http" && is_loopback(&oidc.redirect_uri))
            })
        {
            return Err(Error::InsecureCookie {
                key: String::from("session.secure_cookie"),
            });
        }
        Ok(Settings {
            listen,
            locale,
            bridge: BridgeSettings { base, timeout },
            session,
            oidc,
            telemetry: self.telemetry.clone(),
        })
    }
}

/// Resolves `[oidc]`: the section's check, which refuses a provider URL off
/// TLS, a redirection endpoint that cannot be one and scopes that are not
/// OpenID Connect's, and reads the client secret, then the one rule the
/// console adds for its sign-out.
fn resolve_oidc(oidc: &OidcSection) -> Result<OidcSettings, Error> {
    let checked = oidc.check_with_tls(&ferrohealth_smart::config::ServerTls)?;
    let redirect_uri = Url::parse(oidc.redirect_uri.expose()).map_err(|source| Error::Url {
        key: String::from("oidc.redirect_uri"),
        source,
    })?;
    let endpoints_given = oidc.authorization_endpoint.is_some()
        && oidc.token_endpoint.is_some()
        && oidc.jwks_uri.is_some();
    // NOTE: OpenID Connect RP-Initiated Logout 1.0 §3: the post-logout redirect is sent to
    // the end-session endpoint, so one the written endpoints lack is refused before discovery.
    if oidc.post_logout_redirect_uri.is_some()
        && endpoints_given
        && oidc.end_session_endpoint.is_none()
    {
        return Err(Error::Missing {
            key: String::from("oidc.end_session_endpoint (for post_logout_redirect_uri)"),
        });
    }
    Ok(OidcSettings {
        checked,
        issuer: oidc.issuer.expose().to_owned(),
        redirect_uri,
        endpoints_given,
    })
}

/// Returns `value`, refusing zero.
fn positive(key: &str, value: ferrohealth_config::units::Duration) -> Result<Duration, Error> {
    if value.is_zero() {
        Err(Error::Zero {
            key: key.to_owned(),
        })
    } else {
        Ok(value.get())
    }
}

/// Parses an `http` or `https` URL with a host and no userinfo.
fn web_url(key: &str, text: &str) -> Result<Url, Error> {
    if text.trim().is_empty() {
        return Err(Error::Missing {
            key: key.to_owned(),
        });
    }
    let url = Url::parse(text).map_err(|source| Error::Url {
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
    Ok(url)
}

/// Parses an `http` or `https` URL with a host, no userinfo, no query and no
/// fragment, and ends its path in `/`.
fn base_url(key: &str, text: &str) -> Result<Url, Error> {
    let mut url = web_url(key, text)?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(Error::UrlShape {
            key: key.to_owned(),
            reason: "must not carry a query or a fragment",
        });
    }
    // NOTE: RFC 3986 §5.2.3: a reference merges with the base path up to its
    // last `/`, so the base ends in one or its last segment would be dropped.
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

/// Whether `url` names a loopback host: `localhost`, `127.0.0.0/8` or `::1`.
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(name)) => name == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}
