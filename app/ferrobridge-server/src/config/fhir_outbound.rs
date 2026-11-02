// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[fhir_outbound]` into the settings of the outbound
//! FHIR lane.
//!
//! No specification governs the lane: our own design.

use std::time::Duration;

use super::Config;
use super::credentials::Upstream;
use super::credentials::resolve_credentials;
use super::error::Error;
use super::resolve::url_of;
use crate::change_feed::ReaderName;
use crate::outbound::Settings;
use crate::outbound::sink::Credentials;
use crate::outbound::sink::Sink;
use ferrohealth_smart::client::cache::TokenSource;

/// Returns the lane `config` turns on, refused without the change feed or
/// the facade whose programs it maps with, without a reader name, an
/// endpoint or a delivery record, under another consumer's reader, or with
/// an endpoint that is the facade itself; `None` while it is off.
pub(super) fn resolve(config: &Config) -> Result<Option<Settings>, Error> {
    let section = &config.fhir_outbound;
    if !section.enabled {
        return Ok(None);
    }
    for (on, needed) in [
        (config.change_feed.enabled, "change_feed.enabled"),
        (config.facade.enabled, "facade.enabled"),
    ] {
        if !on {
            return Err(Error::Needs {
                key: String::from("fhir_outbound.enabled"),
                needs: String::from(needed),
            });
        }
    }
    if section.reader.is_empty() {
        return Err(Error::Missing {
            key: String::from("fhir_outbound.reader"),
        });
    }
    let reader = ReaderName::new(&section.reader).map_err(|source| Error::ReaderName {
        key: String::from("fhir_outbound.reader"),
        source,
    })?;
    let taken = [
        (config.events.enabled, config.events.reader.as_str()),
        (
            config.omop_incremental.enabled,
            config.omop_incremental.reader.as_str(),
        ),
    ];
    if taken
        .iter()
        .any(|(on, other)| *on && *other == section.reader)
    {
        return Err(Error::Duplicate {
            key: String::from("fhir_outbound.reader"),
            value: section.reader.clone(),
        });
    }
    let base = url_of("fhir_outbound.base_url", &section.base_url)?;
    let scheme_refused = match base.scheme() {
        "https" => false,
        "http" => !section.cleartext,
        _ => true,
    };
    if scheme_refused || base.query().is_some() || base.fragment().is_some() {
        return Err(Error::FetchBase {
            key: String::from("fhir_outbound.base_url"),
            value: base.to_string(),
        });
    }
    // NOTE: no specification governs this: our own design; a lane that posts to the facade
    // would commit each event back into the CDR it came from.
    if base.as_str().trim_end_matches('/') == config.facade.base_url.trim_end_matches('/') {
        return Err(Error::Duplicate {
            key: String::from("fhir_outbound.base_url"),
            value: base.to_string(),
        });
    }
    if section.timeout_ms == 0 {
        return Err(Error::Bound {
            key: String::from("fhir_outbound.timeout_ms"),
            value: 0,
            reason: "a delivery waits at least one millisecond",
        });
    }
    let emitted = section.emitted.clone().ok_or(Error::Missing {
        key: String::from("fhir_outbound.emitted"),
    })?;
    let credentials = match resolve_credentials("fhir_outbound.credentials", &section.credentials)?
    {
        Some(Upstream::Basic { user, password }) => Credentials::Basic { user, password },
        Some(Upstream::OAuth(grant)) => {
            Credentials::Bearer(TokenSource::new(*grant).map_err(|source| Error::Client {
                upstream: "fhir_outbound",
                source: Box::new(source),
            })?)
        }
        None => Credentials::None,
    };
    let sink = Sink::new(base, Duration::from_millis(section.timeout_ms), credentials).map_err(
        |source| Error::Client {
            upstream: "fhir_outbound",
            source: Box::new(source),
        },
    )?;
    Ok(Some(Settings {
        reader,
        sink,
        emitted,
    }))
}
