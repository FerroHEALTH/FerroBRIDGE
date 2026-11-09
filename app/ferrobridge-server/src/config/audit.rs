// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[audit]` into the settings of the access-record lane.
//!
//! No specification governs the configuration: our own design.

use ehds_logging::map::CategoryMap;
use ehds_logging::map::Matching;
use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::config::ServerTls;
use ihe_atna::atna::feed::FeedRepository;
use ihe_atna::atna::spool::Bounds;

use super::error::Error;
use super::section::audit::Audit;
use crate::audit::Settings;

/// Returns the lane `audit` describes, refused without the facade it records
/// for, without `[auth]`, whose callers it names, and without a repository, a
/// spool or a category map it can read.
///
/// `system_id` is the facade's, which the bridge names itself by when
/// `[audit] source_id` is empty.
pub(super) fn resolve_audit(
    audit: &Audit,
    facade: bool,
    auth: bool,
    system_id: &str,
) -> Result<Settings, Error> {
    if !facade {
        return Err(Error::Needs {
            key: String::from("audit.enabled"),
            needs: String::from("facade.enabled"),
        });
    }
    // NOTE: Regulation (EU) 2025/327 Annex II 3.2(b) asks for the natural person who
    // accessed, which an open surface never names, so the lane needs `[auth]`.
    if !auth {
        return Err(Error::Needs {
            key: String::from("audit.enabled"),
            needs: String::from("auth.enabled"),
        });
    }
    let client =
        OutboundClient::build(&audit.endpoint, "audit.endpoint", &ServerTls).map_err(|source| {
            Error::Client {
                upstream: "audit",
                source: Box::new(source),
            }
        })?;
    let feed = if audit.cleartext {
        FeedRepository::cleartext(client)
    } else {
        FeedRepository::new(client)
    }
    .map_err(|source| Error::AuditBase {
        key: String::from("audit.endpoint.url"),
        source,
    })?;
    let spool = audit.spool_dir.clone().ok_or(Error::Missing {
        key: String::from("audit.spool_dir"),
    })?;
    for (key, zero) in [
        ("audit.write_timeout", audit.write_timeout.is_zero()),
        ("audit.max_size", audit.max_size.is_zero()),
    ] {
        if zero {
            return Err(Error::Bound {
                key: key.to_owned(),
                value: 0,
                reason: "it bounds every record above zero",
            });
        }
    }
    if audit.max_records == 0 {
        return Err(Error::Bound {
            key: String::from("audit.max_records"),
            value: 0,
            reason: "the spool holds at least one record",
        });
    }
    // NOTE: openEHR BASE Release 1.3.0, Identification Package, "Composite Identifiers and
    // Case": template and archetype ids compare case-insensitively.
    let categories = CategoryMap::declare_with(
        Matching::OpenEhr,
        &audit.categories.national,
        &audit.categories.templates,
        &audit.categories.archetypes,
    )
    .map_err(|source| Error::Categories {
        key: String::from("audit.categories"),
        source,
    })?;
    Ok(Settings {
        feed,
        spool,
        bounds: Bounds {
            max_messages: audit.max_records,
            max_bytes: audit.max_size.bytes(),
            write_timeout: audit.write_timeout.get(),
        },
        source_id: if audit.source_id.is_empty() {
            system_id.to_owned()
        } else {
            audit.source_id.clone()
        },
        site: audit.site.clone().filter(|site| !site.is_empty()),
        categories,
    })
}
