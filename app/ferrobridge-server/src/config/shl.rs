// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[shl]` into the settings of the links lane.

use super::Config;
use super::error::Error;
use crate::shl::Settings;

/// The longest manifest URL: it "SHALL NOT exceed 128 characters"
/// (SMART Health Cards and Links 1.0.0, `links-specification`).
const URL_MAX: usize = 128;

/// The characters of a manifest id.
const ID_LENGTH: usize = 43;

/// The longest a file location may live: "SHALL NOT exceed one hour".
const LOCATION_MAX_SECONDS: u64 = 3600;

/// Returns the lane `config` turns on, refused without the patient summary
/// and inbound authentication, without a public origin, with an
/// origin that leaves no room for a manifest id, or with limits outside
/// their range; `None` while it is off.
pub(super) fn resolve(config: &Config) -> Result<Option<Settings>, Error> {
    let section = &config.shl;
    if !section.enabled {
        return Ok(None);
    }
    for (on, needed) in [
        (config.facade.summary.enabled, "facade.summary.enabled"),
        (config.auth.enabled, "auth.enabled"),
    ] {
        if !on {
            return Err(Error::Needs {
                key: String::from("shl.enabled"),
                needs: String::from(needed),
            });
        }
    }
    let base = url::Url::parse(&section.base_url).map_err(|source| Error::Url {
        key: String::from("shl.base_url"),
        source,
    })?;
    let origin = section.base_url.trim_end_matches('/').to_owned();
    if base.scheme() != "https" && base.scheme() != "http"
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(Error::FetchBase {
            key: String::from("shl.base_url"),
            value: section.base_url.clone(),
        });
    }
    let length = origin.len() + crate::shl::MANIFEST.len() + ID_LENGTH;
    if length > URL_MAX {
        return Err(Error::Bound {
            key: String::from("shl.base_url"),
            value: u64::try_from(length).unwrap_or(u64::MAX),
            reason: "a link's URL would pass the 128 characters a SMART Health Link allows",
        });
    }
    let default_expiry = section.default_expiry.get();
    let max_expiry = section.max_expiry.get();
    if default_expiry.is_zero() || default_expiry > max_expiry {
        return Err(Error::Bound {
            key: String::from("shl.default_expiry"),
            value: default_expiry.as_secs(),
            reason: "a link's default life is above zero and at most shl.max_expiry",
        });
    }
    for (key, value) in [
        ("shl.max_answers", section.max_answers),
        ("shl.passcode_attempts", section.passcode_attempts),
    ] {
        if value == 0 {
            return Err(Error::Bound {
                key: String::from(key),
                value: 0,
                reason: "a link gives at least one",
            });
        }
    }
    let location_lifetime = section.location_lifetime.get();
    if location_lifetime.is_zero() || location_lifetime.as_secs() > LOCATION_MAX_SECONDS {
        return Err(Error::Bound {
            key: String::from("shl.location_lifetime"),
            value: location_lifetime.as_secs(),
            reason: "a file location lives above zero and at most one hour",
        });
    }
    Ok(Some(Settings {
        base_url: origin,
        default_expiry,
        max_expiry,
        max_answers: section.max_answers,
        passcode_attempts: section.passcode_attempts,
        location_lifetime,
    }))
}
