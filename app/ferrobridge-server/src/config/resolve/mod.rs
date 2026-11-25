// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of each section of the tree into the settings the run path
//! holds: one file per section, each a `resolve(&Config)` that answers the
//! section's settings or `None` when it is off, and the one refusal of a
//! section set without a neighbour it needs ([`needs`]).

use super::error::Error;

pub(super) mod audit;
pub(super) mod auth;
pub(super) mod cdm;
pub(super) mod cdr;
pub(super) mod change_feed;
pub(super) mod etl;
pub(super) mod events;
pub(super) mod facade;
pub(super) mod fhir_outbound;
pub(super) mod hl7v2;
pub(super) mod inbound;
pub(super) mod mappings;
pub(super) mod omop_incremental;
pub(super) mod operations;
pub(super) mod operator;
pub(super) mod shl;
pub(super) mod state;
pub(super) mod terminology;

/// Refuses `key` while a section it needs is off: the first of `neighbours`
/// whose flag is false, named as the refusal names it.
///
/// # Errors
///
/// Returns [`Error::Needs`] naming `key` and that neighbour.
pub(super) fn needs(key: &str, neighbours: &[(bool, &str)]) -> Result<(), Error> {
    match neighbours.iter().find(|&&(on, _)| !on) {
        Some(&(_, needed)) => Err(needed_by(key, needed)),
        None => Ok(()),
    }
}

/// Returns the refusal of `key` set without `needed`.
pub(super) fn needed_by(key: &str, needed: &str) -> Error {
    Error::Needs {
        key: key.to_owned(),
        needs: needed.to_owned(),
    }
}

/// Returns the value of the required integer `key`.
pub(super) fn required(key: &str, value: Option<i32>) -> Result<i32, Error> {
    value.ok_or_else(|| Error::Missing {
        key: key.to_owned(),
    })
}

/// Returns the URL `value` holds, naming `key` when it does not parse.
pub(super) fn url_of(key: &str, value: &str) -> Result<url::Url, Error> {
    if value.is_empty() {
        return Err(Error::Missing {
            key: key.to_owned(),
        });
    }
    value.parse().map_err(|source| Error::Url {
        key: key.to_owned(),
        source,
    })
}
