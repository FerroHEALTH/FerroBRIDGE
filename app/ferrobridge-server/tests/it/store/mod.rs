// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The state store over a PostgreSQL 19 container: the migrations at start,
//! the lane lease with its fencing generation, and the `LISTEN` session.
//! The cases run only when `FERROBRIDGE_E2E=1` admits the harness.

mod identity;
mod lease;
mod listen;
mod migrate;

use std::time::Duration;

use ferrobridge_server::store::seal::{KeyId, LookupKey, SealKeys, SealingKey};
use ferrobridge_server::store::{Settings, Store};

/// The settings of a store on the database `url` reaches.
fn settings(url: &str) -> Result<Settings, Box<dyn std::error::Error>> {
    Ok(Settings {
        connection: omop_cdm::connection::CdmConnection::new(url, None)?,
        max_connections: 4,
        acquire_timeout: Duration::from_secs(5),
        seal: None,
    })
}

/// A store on `url`, migrated.
async fn open(url: &str) -> Result<Store, Box<dyn std::error::Error>> {
    Ok(Store::open(&settings(url)?).await?)
}

/// The base64 of 32 bytes of `A`, made for the tests.
const KEY_A: &str = "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=";

/// The base64 of 32 bytes of `B`, made for the tests.
const KEY_B: &str = "QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI=";

/// Sealing keys with `lookup` as the lookup key's base64.
fn sealed(lookup: &str) -> Result<SealKeys, Box<dyn std::error::Error>> {
    Ok(SealKeys::new(
        (KeyId::new("test")?, SealingKey::from_base64(KEY_A)?),
        Vec::new(),
        LookupKey::new(&SealingKey::from_base64(lookup)?),
    ))
}

/// A store on `url` with sealing keys whose lookup key is `lookup`, migrated.
async fn open_sealed(url: &str, lookup: &str) -> Result<Store, Box<dyn std::error::Error>> {
    let mut settings = settings(url)?;
    settings.seal = Some(sealed(lookup)?);
    Ok(Store::open(&settings).await?)
}
