// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[identity_source]` into the `PIXm` Consumer every EHR
//! subject is asked through.

use super::error::Error;
use super::section::identity_source::IdentitySource as Section;
use super::section::identity_source::PseudonymCache as CacheSection;
use crate::identity_source::cache::Bounds;
use crate::identity_source::subscription::Subscription;
use crate::identity_source::{CacheSettings, IdentitySource};
use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::config::ServerTls;
use ihe_iti::pixm::PixmClient;
use ihe_iti::pixm::identifier::TargetSystem;
use ihe_iti::pixm::source::PixmSourceClient;
use ihe_iti::pmir::PmirSubscriber;
use ihe_iti::pmir::subscription::{Criteria, SubscriptionRequest};

/// Returns the identity source `section` names.
///
/// It is refused without a pseudonym domain or a source domain, with a
/// domain that is no absolute URI, with a domain named twice, or with a base
/// that is no `http` or `https` base.
///
/// # Errors
/// Returns [`Error::Missing`], [`Error::IdentityDomain`],
/// [`Error::Duplicate`], [`Error::Client`] or [`Error::FetchBase`], each
/// naming the key at fault.
pub fn resolve(section: &Section) -> Result<IdentitySource, Error> {
    if section.pseudonym_domain.is_empty() {
        return Err(Error::Missing {
            key: String::from("identity_source.pseudonym_domain"),
        });
    }
    if section.source_domains.is_empty() {
        return Err(Error::Missing {
            key: String::from("identity_source.source_domains"),
        });
    }
    let pseudonym_domain =
        TargetSystem::new(section.pseudonym_domain.clone()).map_err(|_invalid| {
            Error::IdentityDomain {
                key: String::from("identity_source.pseudonym_domain"),
                value: section.pseudonym_domain.clone(),
            }
        })?;
    for (index, domain) in section.source_domains.iter().enumerate() {
        TargetSystem::new(domain.clone()).map_err(|_invalid| Error::IdentityDomain {
            key: String::from("identity_source.source_domains"),
            value: domain.clone(),
        })?;
        if section
            .source_domains
            .iter()
            .take(index)
            .any(|seen| seen == domain)
        {
            return Err(Error::Duplicate {
                key: String::from("identity_source.source_domains"),
                value: domain.clone(),
            });
        }
    }
    let client = OutboundClient::build(&section.endpoint, "identity_source.endpoint", &ServerTls)
        .map_err(|source| Error::Client {
        upstream: "identity_source",
        source: Box::new(source),
    })?;
    let not_a_base = |_invalid| Error::FetchBase {
        key: String::from("identity_source.endpoint.url"),
        value: client.url().to_string(),
    };
    let feed = PixmSourceClient::new(client.clone()).map_err(not_a_base)?;
    let pixm = PixmClient::new(client.clone()).map_err(not_a_base)?;
    let cache = section
        .cache
        .as_ref()
        .map(|cache| resolve_cache(cache, &client, section.endpoint.timeout.into()))
        .transpose()?;
    Ok(IdentitySource::new(
        (pixm, feed),
        pseudonym_domain,
        section.source_domains.clone(),
        section.endpoint.timeout.into(),
    )
    .with_cache_settings(cache))
}

/// Returns what `cache` configures over the identity source's `client`.
///
/// It is refused without entries, with a zero bound, with a channel endpoint
/// that is no `http` or `https` URL, or without a client the receiver
/// admits.
fn resolve_cache(
    cache: &CacheSection,
    client: &OutboundClient,
    timeout: std::time::Duration,
) -> Result<CacheSettings, Error> {
    if cache.entries == 0 {
        return Err(Error::Zero {
            key: String::from("identity_source.cache.entries"),
        });
    }
    for (key, bound) in [
        ("identity_source.cache.max_age", cache.max_age),
        ("identity_source.cache.status_every", cache.status_every),
    ] {
        if bound.is_zero() {
            return Err(Error::Bound {
                key: String::from(key),
                value: 0,
                reason: "it is longer than no time",
            });
        }
    }
    if cache.feed_url.is_empty() {
        return Err(Error::Missing {
            key: String::from("identity_source.cache.feed_url"),
        });
    }
    if cache.feed_clients.is_empty() {
        return Err(Error::Missing {
            key: String::from("identity_source.cache.feed_clients"),
        });
    }
    let endpoint = url::Url::parse(&cache.feed_url).map_err(|source| Error::Url {
        key: String::from("identity_source.cache.feed_url"),
        source,
    })?;
    let request =
        SubscriptionRequest::new(Criteria::AllPatients, endpoint).map_err(|_invalid| {
            Error::FetchBase {
                key: String::from("identity_source.cache.feed_url"),
                value: cache.feed_url.clone(),
            }
        })?;
    let subscriber = PmirSubscriber::new(client.clone()).map_err(|_invalid| Error::FetchBase {
        key: String::from("identity_source.endpoint.url"),
        value: client.url().to_string(),
    })?;
    Ok(CacheSettings {
        bounds: Bounds {
            entries: cache.entries,
            max_age: cache.max_age.into(),
            status_every: cache.status_every.into(),
        },
        subscription: Subscription {
            subscriber,
            request,
            every: cache.status_every.into(),
            timeout,
        },
        feed_url: cache.feed_url.clone(),
        feed_clients: cache.feed_clients.clone(),
        registry: client.url().clone(),
        recorder: None,
    })
}
