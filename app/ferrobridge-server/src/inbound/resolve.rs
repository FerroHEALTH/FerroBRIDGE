// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A webhook source's declared resolution: the `GET` calls that read the
//! entity a notification names, and the map that renders their answers into
//! an R4 `Patient`.
//!
//! Each call is a path below the tenant's configured base in which only
//! `{entity}` is replaced, percent-encoded, so no call leaves the base's
//! scheme, host and path. A call presents the tenant's client certificate
//! through the source's client in `inbound::clients`, and an answer is read
//! as JSON up to the declared size. The answers are the map's sources, in
//! call order (FHIR R4 §7.7, mapping-language.html). No specification
//! governs an inbound resolution: our own design.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use fhir_types::codec::Value;
use serde::Deserialize;

use super::apply::{Resolve, Resolved, Step};
use super::clients::Clients;
use super::recover::RecoverError;
use super::worker::Failed;
use crate::retry::Failure;
use crate::sources::document::{Document, Invalid, Tenant};
use crate::sources::{Source, Sources};
use crate::structuremap::engine::{Engine, MapError};
use crate::structuremap::syntax::InputMode;

/// The one placeholder a call's path may carry.
const ENTITY: &str = "{entity}";

/// The headers a call may not carry: a credential is the tenant's client
/// certificate, never a value in the document.
const CREDENTIAL_HEADERS: [&str; 4] = [
    "authorization",
    "proxy-authorization",
    "cookie",
    "x-api-key",
];

/// The declared resolution of a webhook source.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    /// The calls, in order; each answer is one source of the map.
    pub calls: Vec<Call>,
    /// The status of the first call that reads as the entity removed.
    #[serde(default)]
    pub removed_on: Option<u16>,
    /// The canonical URL of the map that renders the answers into an R4
    /// `Patient`.
    pub map: String,
    /// The largest answer read, in bytes; 256 KiB when left out.
    #[serde(default)]
    pub answer_limit: Option<usize>,
}

/// One `GET` call.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
    /// The path and query below the tenant's base, `{entity}` standing for
    /// the entity's id.
    pub path: String,
    /// The headers the call sends beside `Accept: application/json`.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

impl Resolution {
    /// Returns the largest answer read, in bytes.
    #[must_use]
    pub fn answer_limit(&self) -> usize {
        self.answer_limit.unwrap_or(256 * 1024)
    }

    /// Refuses a resolution whose calls, status, map or limit break a rule,
    /// or whose calls would leave the base of one of `tenants`.
    ///
    /// # Errors
    /// Returns [`Invalid`] naming the first broken rule.
    pub fn check(&self, tenants: &[Tenant]) -> Result<(), Invalid> {
        if self.calls.is_empty() {
            return Err(Invalid(String::from("resolve names no call")));
        }
        for call in &self.calls {
            call.check()?;
        }
        if let Some(status) = self.removed_on {
            let refusal = http::StatusCode::from_u16(status)
                .ok()
                .filter(|status| status.is_client_error())
                .filter(|status| super::STATUSES.failure(*status) == Failure::Poisoned);
            if refusal.is_none() {
                return Err(Invalid(format!(
                    "resolve.removed_on {status} is no client error status that parks"
                )));
            }
        }
        if url::Url::parse(&self.map).is_err() {
            return Err(Invalid(format!("resolve.map {} is no URL", self.map)));
        }
        if !(1..=16 * 1024 * 1024).contains(&self.answer_limit()) {
            return Err(Invalid(String::from(
                "resolve.answer_limit is 1 byte to 16 MiB",
            )));
        }
        for tenant in tenants {
            let base = tenant.base.as_deref().ok_or_else(|| {
                Invalid(format!(
                    "the tenant {} names no base for the resolve calls",
                    tenant.name
                ))
            })?;
            let base = base_url(base).map_err(|reason| {
                Invalid(format!("the base of the tenant {}: {reason}", tenant.name))
            })?;
            for call in &self.calls {
                if call_url(&base, &call.path, "entity").is_none() {
                    return Err(Invalid(format!(
                        "the call {} leaves the base of the tenant {}",
                        call.path, tenant.name
                    )));
                }
            }
        }
        Ok(())
    }

    /// Refuses this resolution when `engine` runs no map at its URL, or the
    /// map's first group does not take one source per call and a target.
    ///
    /// # Errors
    /// Returns [`Invalid`] naming the map.
    pub fn runs_on(&self, engine: &Engine) -> Result<(), Invalid> {
        let map = engine.map(&self.map).ok_or_else(|| {
            Invalid(format!(
                "resolve.map {} is no map of the running revision",
                self.map
            ))
        })?;
        let group = map.groups.first();
        let count = |mode| {
            group.map_or(0, |group| {
                group
                    .parameters
                    .iter()
                    .filter(|parameter| parameter.mode == mode)
                    .count()
            })
        };
        if count(InputMode::Source) != self.calls.len() || count(InputMode::Target) != 1 {
            return Err(Invalid(format!(
                "the first group of {} takes one source per call and one target",
                self.map
            )));
        }
        Ok(())
    }
}

impl Call {
    /// Refuses a path that is not below the base or carries another
    /// placeholder, and a header that does not read or carries a credential.
    fn check(&self) -> Result<(), Invalid> {
        let path = &self.path;
        let rest = path.replace(ENTITY, "");
        if !path.starts_with('/')
            || path.starts_with("//")
            || !path.contains(ENTITY)
            || rest.contains(['{', '}', '#', '\\'])
            || rest.contains("://")
        {
            return Err(Invalid(format!(
                "the call {path} is no path below the base with {ENTITY} and no other placeholder"
            )));
        }
        for (name, value) in &self.headers {
            let parsed = http::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_name| Invalid(format!("the header {name} does not read")))?;
            if CREDENTIAL_HEADERS.contains(&parsed.as_str()) {
                return Err(Invalid(format!(
                    "the header {name} carries a credential; a call presents the tenant's certificate"
                )));
            }
            if http::HeaderValue::from_str(value).is_err() {
                return Err(Invalid(format!(
                    "the value of the header {name} does not read"
                )));
            }
        }
        Ok(())
    }
}

/// Returns the base `text` names, or why it names none.
fn base_url(text: &str) -> Result<url::Url, &'static str> {
    let base = url::Url::parse(text).map_err(|_url| "no URL")?;
    if !matches!(base.scheme(), "http" | "https") || base.host_str().is_none() {
        return Err("no HTTP URL");
    }
    if !base.username().is_empty() || base.password().is_some() {
        return Err("a base carries no credentials");
    }
    if base.query().is_some() || base.fragment().is_some() {
        return Err("a base carries no query and no fragment");
    }
    Ok(base)
}

/// Returns the URL of `path` below `base` for `entity`, `None` when it
/// would leave the base's scheme, host, port or path.
fn call_url(base: &url::Url, path: &str, entity: &str) -> Option<url::Url> {
    if entity.is_empty() || entity.chars().all(|character| character == '.') {
        return None;
    }
    let prefix = base.path().trim_end_matches('/');
    let text = format!(
        "{}://{}{}{}",
        base.scheme(),
        base.host_str()?,
        base.port()
            .map(|port| format!(":{port}"))
            .unwrap_or_default(),
        prefix,
    ) + &path.replace(ENTITY, &encoded(entity));
    let url = url::Url::parse(&text).ok()?;
    let same = url.scheme() == base.scheme()
        && url.host_str() == base.host_str()
        && url.port_or_known_default() == base.port_or_known_default()
        && url.username().is_empty()
        && url.path().starts_with(prefix)
        && url
            .path()
            .get(prefix.len()..)
            .is_some_and(|rest| rest.starts_with('/'));
    same.then_some(url)
}

/// Returns `text` with every byte outside the RFC 3986 §2.3 unreserved set,
/// and `.`, percent-encoded (RFC 3986 §2.1).
fn encoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The resolution every source runs: its declared calls and map, or a park
/// as `resolution-undeclared` for a source that declares none.
#[derive(Debug)]
pub struct Declared {
    /// The sources, for the tenant's credentials and the running maps.
    sources: Arc<Sources>,
    /// The HTTP client of each source and credential.
    clients: Clients,
}

impl Declared {
    /// Returns the resolution over `sources`.
    #[must_use]
    pub fn new(sources: Arc<Sources>) -> Self {
        Self {
            sources,
            clients: Clients::default(),
        }
    }

    /// Reads every answer of `resolution` for `entity` of `tenant`, `None`
    /// when the first call answers `removed_on`.
    async fn answers(
        &self,
        source: &Source,
        resolution: &Resolution,
        tenant: &Tenant,
        entity: &str,
    ) -> Result<Option<Vec<Value>>, Failed> {
        let base = tenant
            .base
            .as_deref()
            .and_then(|base| base_url(base).ok())
            .ok_or_else(|| refused("tenant-base-undeclared"))?;
        let mut answers = Vec::with_capacity(resolution.calls.len());
        for (index, call) in resolution.calls.iter().enumerate() {
            let url =
                call_url(&base, &call.path, entity).ok_or_else(|| refused("entity-invalid"))?;
            let client = self
                .clients
                .client(&self.sources, source, tenant.credentials.as_ref(), &url)
                .map_err(|error| match error {
                    RecoverError::Identity(_) => refused("credentials-invalid"),
                    _ => transient("credentials-unavailable"),
                })?;
            let answer = client
                .send(&http::Method::GET, &url, |mut request| {
                    request = request.header(http::header::ACCEPT, "application/json");
                    for (name, value) in &call.headers {
                        request = request.header(name.as_str(), value.as_str());
                    }
                    request
                })
                .await
                .map_err(|_unanswered| transient("source-unavailable"))?;
            let status = answer.status();
            if index == 0 && resolution.removed_on == Some(status.as_u16()) {
                return Ok(None);
            }
            if !status.is_success() {
                return Err(match super::STATUSES.failure(status) {
                    Failure::Transient => transient("source-unavailable"),
                    Failure::Poisoned => Failed {
                        failure: Failure::Poisoned,
                        reason: Cow::Owned(format!("source-answered-{}", status.as_u16())),
                    },
                });
            }
            answers.push(read(answer, resolution.answer_limit()).await?);
        }
        Ok(Some(answers))
    }
}

impl Resolve for Declared {
    fn patient<'a>(
        &'a self,
        source: &'a Source,
        tenant: &'a Tenant,
        entity: &'a str,
    ) -> Step<'a, Resolved> {
        Box::pin(async move {
            let Document::Webhook(webhook) = &source.parsed else {
                return Err(refused("resolution-undeclared"));
            };
            let resolution = webhook
                .resolve
                .as_ref()
                .ok_or_else(|| refused("resolution-undeclared"))?;
            let Some(answers) = self.answers(source, resolution, tenant, entity).await? else {
                return Ok(Resolved::Removed);
            };
            let engine = self.sources.structuremaps().current();
            if engine.map(&resolution.map).is_none() {
                return Err(refused("map-unknown"));
            }
            let output = engine
                .transform_blocking(resolution.map.clone(), answers, String::from("Patient"))
                .await
                .map_err(|error| map_failed(&error))?;
            Ok(Resolved::Patient(output.target))
        })
    }
}

/// Reads the JSON answer `answer`, refusing one larger than `limit` bytes.
async fn read(mut answer: reqwest::Response, limit: usize) -> Result<Value, Failed> {
    let declared = answer
        .content_length()
        .and_then(|length| usize::try_from(length).ok());
    if declared.is_some_and(|length| length > limit) {
        return Err(refused("answer-too-large"));
    }
    let mut body = Vec::with_capacity(declared.unwrap_or(0).min(limit));
    while let Some(chunk) = answer
        .chunk()
        .await
        .map_err(|_unread| transient("source-unavailable"))?
    {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(refused("answer-too-large"));
        }
        body.extend_from_slice(&chunk);
    }
    let json: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_json| refused("answer-unreadable"))?;
    Ok(Value::from_serde_json(json))
}

/// Returns how a failed map run fails the item: a `translate` the
/// terminology server did not answer, or answered with a `5xx`, `429` or
/// `423`, runs again, and every other failure parks.
fn map_failed(error: &MapError) -> Failed {
    match error {
        MapError::Translate { source, .. }
            if source
                .status()
                .is_none_or(|status| super::STATUSES.failure(status) == Failure::Transient) =>
        {
            transient("terminology-unavailable")
        }
        MapError::Interrupted => transient("map-interrupted"),
        _ => refused("map-failed"),
    }
}

/// Returns the failure for good of `reason`.
const fn refused(reason: &'static str) -> Failed {
    Failed {
        failure: Failure::Poisoned,
        reason: Cow::Borrowed(reason),
    }
}

/// Returns the failure of `reason` that runs again.
const fn transient(reason: &'static str) -> Failed {
    Failed {
        failure: Failure::Transient,
        reason: Cow::Borrowed(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::{base_url, call_url, encoded};

    #[test]
    fn only_the_entity_is_replaced_and_no_call_leaves_the_base() {
        let base = base_url("https://api.example/tenant-1/").expect("a base");
        let url = call_url(&base, "/v0/clients/{entity}/bsn", "a b/../c").expect("a call");
        assert_eq!(
            "https://api.example/tenant-1/v0/clients/a%20b%2F%2E%2E%2Fc/bsn",
            url.as_str()
        );
        for entity in ["", ".", ".."] {
            assert!(call_url(&base, "/v0/clients/{entity}", entity).is_none());
        }
        assert!(call_url(&base, "/../other/{entity}", "1").is_none());
        assert!(call_url(&base, "@evil.example/{entity}", "1").is_none());
    }

    #[test]
    fn a_base_is_an_http_url_without_credentials_query_or_fragment() {
        for text in [
            "ftp://api.example",
            "https://user:pass@api.example",
            "https://api.example/?q=1",
            "https://api.example/#f",
            "not a url",
        ] {
            assert!(base_url(text).is_err(), "{text}");
        }
        assert_eq!("%C3%A9%2E", encoded("é."));
    }
}
