// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[fhir_outbound]` into the settings of the outbound
//! FHIR lane.
//!
//! No specification governs the lane: our own design.

use super::Config;
use super::change_feed::Consumer;
use super::error::Error;
use crate::outbound::Settings;
use crate::outbound::sink::Sink;
use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::config::ServerTls;

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
    let reader = Consumer::FhirOutbound.reader(config)?;
    let client = OutboundClient::build(&section.endpoint, "fhir_outbound.endpoint", &ServerTls)
        .map_err(|source| Error::Client {
            upstream: "fhir_outbound",
            source: Box::new(source),
        })?;
    let base = client.url().clone();
    let scheme_refused = match base.scheme() {
        "https" => false,
        "http" => !section.cleartext,
        _ => true,
    };
    if scheme_refused || base.query().is_some() || base.fragment().is_some() {
        return Err(Error::FetchBase {
            key: String::from("fhir_outbound.endpoint.url"),
            value: base.to_string(),
        });
    }
    // NOTE: no specification governs this: our own design; a lane that posts to the facade
    // would commit each event back into the CDR it came from.
    if base.as_str().trim_end_matches('/') == config.facade.base_url.trim_end_matches('/') {
        return Err(Error::Duplicate {
            key: String::from("fhir_outbound.endpoint.url"),
            value: base.to_string(),
        });
    }
    let sink = Sink::new(client);
    Ok(Some(Settings { reader, sink }))
}
