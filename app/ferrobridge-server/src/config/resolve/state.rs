// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[state]` into the settings of the state store.

use secrecy::ExposeSecret as _;

use crate::config::Config;
use crate::config::credentials::{ConnectionKeys, secret};
use crate::config::error::Error;
use crate::config::section::state::{State, StateSeal};
use crate::store::Settings;
use crate::store::seal::{KeyId, LookupKey, SealKeys, SealingKey};

/// Returns the store `state` describes, its URL, CA and sealing keys read.
fn settings(state: &State) -> Result<Settings, Error> {
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
    let session = omop_cdm::connection::session::Session {
        statement_timeout: state.statement_timeout.get(),
        lock_timeout: state.lock_timeout.get(),
        idle_in_transaction_session_timeout: state.idle_in_transaction_session_timeout.get(),
        statement_cache_capacity: state.statement_cache_capacity,
        max_lifetime: state.max_lifetime.get(),
        idle_timeout: state.idle_timeout.get(),
    };
    let connection = keys.connection(
        url.expose_secret(),
        ca.as_deref(),
        session,
        |key, source| Error::StateConnection { key, source },
    )?;
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
    let lookup = secret(
        "state.seal.lookup_key",
        seal.lookup_key.as_ref(),
        seal.lookup_key_file.as_deref(),
    )?
    .ok_or(Error::Missing {
        key: String::from("state.seal.lookup_key"),
    })?;
    let lookup =
        SealingKey::from_base64(lookup.expose_secret()).map_err(|source| Error::SealKey {
            key: String::from("state.seal.lookup_key"),
            source,
        })?;
    Ok(SealKeys::new(
        current,
        keys.collect(),
        LookupKey::new(&lookup),
    ))
}

/// Returns the state store of `[state]`, when it is present, boxed so the
/// settings stay small enough to move through a job's future.
///
/// # Errors
///
/// Returns the [`Error`] of the section's first fault.
pub(in crate::config) fn resolve(config: &Config) -> Result<Option<Box<Settings>>, Error> {
    config
        .state
        .as_ref()
        .map(|state| settings(state).map(Box::new))
        .transpose()
}
