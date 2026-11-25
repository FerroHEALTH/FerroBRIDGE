// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[identity_source]` into the `PIXm` Consumer every EHR
//! subject is asked through.

use super::error::Error;
use super::section::identity_source::IdentitySource as Section;
use crate::identity_source::IdentitySource;
use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::config::ServerTls;
use ihe_iti::pixm::PixmClient;
use ihe_iti::pixm::identifier::TargetSystem;

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
    let base = client.url().to_string();
    let pixm = PixmClient::new(client).map_err(|_not_a_base| Error::FetchBase {
        key: String::from("identity_source.endpoint.url"),
        value: base,
    })?;
    Ok(IdentitySource::new(
        pixm,
        pseudonym_domain,
        section.source_domains.clone(),
        section.endpoint.timeout.into(),
    ))
}
