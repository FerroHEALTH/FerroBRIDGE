// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[terminology]` into the terminology client.
//!
//! No specification governs the configuration: our own design.

use crate::config::Config;
use crate::config::credentials::resolve_credentials;
use crate::config::error::Error;
use crate::config::section::terminology::Terminology;

/// Returns the terminology client `terminology` describes, built from its
/// `endpoint` table with the family's TLS reader.
fn settings(terminology: &Terminology) -> Result<ferrobridge_term::client::Client, Error> {
    super::url_of(
        "terminology.endpoint.url",
        terminology.endpoint.url.expose(),
    )?;
    // NOTE: no specification governs this: our own design; the credentials are checked by the
    // bridge's rules first, the onward table refused on this lane, before the client takes them.
    let _checked = resolve_credentials(
        "terminology.endpoint.credentials",
        &terminology.endpoint.credentials,
    )?;
    let wire_version = match terminology.wire_version.to_ascii_lowercase().as_str() {
        "stu3" => ferrobridge_term::config::WireVersion::Stu3,
        "r4" => ferrobridge_term::config::WireVersion::R4,
        "r4b" => ferrobridge_term::config::WireVersion::R4B,
        _ => {
            return Err(Error::WireVersion {
                key: String::from("terminology.wire_version"),
                value: terminology.wire_version.clone(),
            });
        }
    };
    ferrobridge_term::client::Client::from_endpoint(
        &terminology.endpoint,
        "terminology.endpoint",
        &ferrohealth_smart::config::ServerTls,
        wire_version,
    )
    .map_err(|source| Error::Client {
        upstream: "terminology",
        source: Box::new(source),
    })
}

/// Returns the terminology client of `[terminology]`, when it is present.
///
/// # Errors
///
/// Returns the [`Error`] of the section's first fault.
pub(in crate::config) fn resolve(
    config: &Config,
) -> Result<Option<ferrobridge_term::client::Client>, Error> {
    config.terminology.as_ref().map(settings).transpose()
}
