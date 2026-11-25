// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[audit]` into the settings of the access-record lane.
//!
//! No specification governs the configuration: our own design.

use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::config::ServerTls;

use crate::audit::Settings;
use crate::config::Config;
use crate::config::error::Error;
use crate::config::section::audit::Audit;

/// Returns the lane `audit` describes, refused without the facade it records
/// for, without `[auth]`, whose callers it names, without `[state]`, which
/// keeps its outbox and its category register, and without a repository it
/// can read.
///
/// `system_id` is the facade's, which the bridge names itself by when
/// `[audit] source_id` is empty.
fn settings(
    audit: &Audit,
    facade: bool,
    auth: bool,
    state: bool,
    system_id: &str,
) -> Result<Settings, Error> {
    // NOTE: Regulation (EU) 2025/327 Annex II 3.2(b) asks for the natural person who
    // accessed, which an open surface never names, so the lane needs `[auth]`.
    super::needs(
        "audit.enabled",
        &[
            (facade, "facade.enabled"),
            (auth, "auth.enabled"),
            (state, "[state]"),
        ],
    )?;
    let client =
        OutboundClient::build(&audit.endpoint, "audit.endpoint", &ServerTls).map_err(|source| {
            Error::Client {
                upstream: "audit",
                source: Box::new(source),
            }
        })?;
    let feed = crate::audit::feed::repository(client, audit.cleartext).map_err(|source| {
        Error::AuditBase {
            key: String::from("audit.endpoint.url"),
            source,
        }
    })?;
    Ok(Settings {
        feed,
        source_id: if audit.source_id.is_empty() {
            system_id.to_owned()
        } else {
            audit.source_id.clone()
        },
        site: audit.site.clone().filter(|site| !site.is_empty()),
    })
}

/// Returns the access-record lane of `[audit]`, when it is on.
///
/// # Errors
///
/// Returns the [`Error`] of the section's first fault.
pub(in crate::config) fn resolve(config: &Config) -> Result<Option<Settings>, Error> {
    if !config.audit.enabled {
        return Ok(None);
    }
    settings(
        &config.audit,
        config.facade.enabled,
        config.auth.enabled,
        config.state.is_some(),
        &config.facade.system_id,
    )
    .map(Some)
}
