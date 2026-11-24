// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[state]` into the settings of the state store.

use secrecy::ExposeSecret as _;

use super::credentials::{ConnectionKeys, secret};
use super::error::Error;
use super::section::state::{State, StateSeal};
use crate::store::Settings;
use crate::store::seal::{KeyId, SealKeys, SealingKey};

/// Returns the store `state` describes, its URL, CA and sealing keys read.
pub(super) fn resolve(state: &State) -> Result<Settings, Error> {
    let url = secret("state.url", state.url.as_ref(), state.url_file.as_deref())?.ok_or(
        Error::Missing {
            key: String::from("state.url"),
        },
    )?;
    let keys = ConnectionKeys {
        section: "state",
        url_file: state.url_file.is_some(),
        tls_ca: state.tls_ca.as_deref(),
        tls_ca_file: state.tls_ca_file.as_deref(),
    };
    let ca = keys.ca()?;
    let connection = omop_cdm::connection::CdmConnection::new(url.expose_secret(), ca.as_deref())
        .map_err(|source| Error::StateConnection {
        key: keys.refused(&source),
        source,
    })?;
    if state.max_connections == 0 {
        return Err(Error::Bound {
            key: String::from("state.max_connections"),
            value: 0,
            reason: "the pool holds at least one connection",
        });
    }
    if state.acquire_timeout.is_zero() {
        return Err(Error::Bound {
            key: String::from("state.acquire_timeout"),
            value: 0,
            reason: "a request waits longer than no time for a connection",
        });
    }
    Ok(Settings {
        connection,
        max_connections: state.max_connections,
        acquire_timeout: state.acquire_timeout.get(),
        seal: state.seal.as_ref().map(resolve_seal).transpose()?,
    })
}

/// Returns the sealing keys `seal` lists, the first sealing.
fn resolve_seal(seal: &StateSeal) -> Result<SealKeys, Error> {
    let mut keys: Vec<(KeyId, SealingKey)> = Vec::with_capacity(seal.keys.len());
    for entry in &seal.keys {
        let id = KeyId::new(&entry.id).map_err(|source| Error::SealKey {
            key: String::from("state.seal.keys.id"),
            source,
        })?;
        if keys.iter().any(|(seen, _)| *seen == id) {
            return Err(Error::Duplicate {
                key: String::from("state.seal.keys.id"),
                value: entry.id.clone(),
            });
        }
        let text = secret(
            "state.seal.keys.key",
            entry.key.as_ref(),
            entry.key_file.as_deref(),
        )?
        .ok_or(Error::Missing {
            key: String::from("state.seal.keys.key"),
        })?;
        let key =
            SealingKey::from_base64(text.expose_secret()).map_err(|source| Error::SealKey {
                key: String::from("state.seal.keys.key"),
                source,
            })?;
        keys.push((id, key));
    }
    let mut keys = keys.into_iter();
    let current = keys.next().ok_or(Error::Missing {
        key: String::from("state.seal.keys"),
    })?;
    Ok(SealKeys::new(current, keys.collect()))
}
