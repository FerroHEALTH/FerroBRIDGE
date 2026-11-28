// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[cdr]` into the CDR client configuration.
//!
//! No specification governs the configuration: our own design.

use crate::cdr::config::CdrCredentials;
use crate::config::Config;
use crate::config::credentials::resolve_credentials;
use crate::config::error::Error;
use crate::config::section::cdr::Cdr;

/// Returns the CDR client configuration `cdr` describes.
fn settings(cdr: &Cdr) -> Result<crate::cdr::config::CdrConfig, Error> {
    let endpoint = &cdr.endpoint;
    let refused = |source| Error::Credentials {
        section: String::from("cdr.endpoint"),
        source: Box::new(source),
    };
    super::url_of("cdr.endpoint.url", endpoint.url.expose())?;
    let base_url = endpoint.validate().map_err(refused)?;
    let tls = ferrohealth_smart::config::TlsBuilder::build(
        &ferrohealth_smart::config::ServerTls,
        "cdr.endpoint.tls",
        &endpoint.tls,
        std::slice::from_ref(&base_url),
    )
    .map_err(|source| Error::Client {
        upstream: "cdr",
        source,
    })?;
    let mut config = crate::cdr::config::CdrConfig::new(base_url)
        .with_timeout(endpoint.timeout.get())
        .with_connect_timeout(
            endpoint
                .connect_timeout
                .map(ferrohealth_config::units::Duration::get),
        )
        .with_tls(tls)
        .with_demographic(cdr.demographic)
        .with_retry(openehr_its::rest::client::RetryPolicy {
            // NOTE: no specification governs this: our own design; a u32 fits a
            // usize on every target the server builds for, so the clamp never bites.
            max_attempts: usize::try_from(endpoint.retry.max_attempts).unwrap_or(usize::MAX),
            initial_backoff: endpoint.retry.initial_backoff.get(),
            max_backoff: endpoint.retry.max_backoff.get(),
        });
    let own = ferrohealth_smart::config::credentials::CredentialsSection {
        onward: None,
        ..endpoint.credentials.clone()
    };
    match resolve_credentials("cdr.endpoint.credentials", &own)? {
        Some(CdrCredentials::Basic { user, password }) => {
            config = config
                .with_basic(user, password)
                .with_committer_issuer(cdr.committer_issuer.clone());
        }
        Some(CdrCredentials::OAuth(_)) | None if cdr.committer_issuer.is_some() => {
            return Err(super::needed_by(
                "cdr.committer_issuer",
                "cdr.endpoint.credentials.user",
            ));
        }
        Some(CdrCredentials::OAuth(grant)) => {
            config = config.with_oauth(*grant);
        }
        None => {}
    }
    let onward = resolve_onward(cdr)?;
    // NOTE: RFC 8707 §2 and Microsoft's `scope=api://<cdr>/.default`: a machine's commit
    // goes under the bridge's own token, which must name the CDR alone.
    if onward.is_some()
        && endpoint
            .credentials
            .oauth
            .as_ref()
            .is_some_and(|oauth| oauth.resource.is_empty() && oauth.scope.is_empty())
    {
        return Err(super::needed_by(
            "cdr.endpoint.credentials.onward",
            "cdr.endpoint.credentials.oauth.resource or cdr.endpoint.credentials.oauth.scope naming the CDR",
        ));
    }
    if let Some(onward) = onward {
        config = config.with_onward(onward);
    }
    Ok(config)
}

/// Returns the onward lane of `cdr`: the issuer of `endpoint.credentials.onward`
/// and each `[[cdr.onward]]` issuer, at most one of each kind, or `None`
/// when it declares none.
fn resolve_onward(cdr: &Cdr) -> Result<Option<crate::cdr::onward::CdrOnward>, Error> {
    let refused = |section: &str, source| Error::Credentials {
        section: String::from(section),
        source: Box::new(source),
    };
    let mut issuers = Vec::new();
    if let (Some(lane), Some(kind)) = (
        cdr.endpoint
            .credentials
            .onward_with_tls(&ferrohealth_smart::config::ServerTls)
            .map_err(|source| refused("cdr.endpoint.credentials", source))?,
        cdr.endpoint
            .credentials
            .onward
            .as_ref()
            .map(|onward| onward.kind.unwrap_or_default()),
    ) {
        issuers.push((lane, kind));
    }
    for section in &cdr.onward {
        let oauth = cdr.endpoint.credentials.oauth.as_ref().ok_or_else(|| {
            refused(
                "cdr.onward",
                ferrohealth_smart::config::ConfigError::Missing {
                    key: "cdr.endpoint.credentials.oauth, which cdr.onward authenticates with",
                },
            )
        })?;
        let lane = oauth
            .resolve_with_tls(&ferrohealth_smart::config::ServerTls)
            .and_then(|credentials| section.resolve(credentials))
            .map_err(|source| refused("cdr.onward", source))?;
        let kind = section.kind.unwrap_or_default();
        if issuers.iter().any(|(_, seen)| *seen == kind) {
            return Err(Error::Duplicate {
                key: String::from("cdr.onward.kind"),
                value: String::from(kind.as_str()),
            });
        }
        issuers.push((lane, kind));
    }
    let mut issuers = issuers.into_iter();
    Ok(issuers.next().map(|(lane, kind)| {
        issuers.fold(
            crate::cdr::onward::CdrOnward::new(lane, kind),
            |onward, (lane, kind)| onward.with_issuer(lane, kind),
        )
    }))
}

/// Returns the CDR client configuration of `[cdr]`, when it is present.
///
/// # Errors
///
/// Returns the [`Error`] of the section's first fault.
pub(in crate::config) fn resolve(
    config: &Config,
) -> Result<Option<crate::cdr::config::CdrConfig>, Error> {
    config.cdr.as_ref().map(settings).transpose()
}
