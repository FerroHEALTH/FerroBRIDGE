// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The state store over a PostgreSQL 19 container: the migrations at start,
//! the lane lease with its fencing generation, and the `LISTEN` session.
//! The cases run only when `FERROBRIDGE_E2E=1` admits the harness.

mod lease;
mod listen;
mod migrate;

use std::time::Duration;

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
