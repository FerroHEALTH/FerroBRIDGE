// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The baselines run once at start, whichever replica starts first.

use std::collections::BTreeMap;
use std::error::Error;

use ferrobridge_server::config::{Config, Process};
use ferrobridge_server::state::AppState;
use ferrobridge_server::store::Store;
use ferrobridge_testkit::containers;

#[tokio::test]
async fn replicas_starting_together_migrate_once() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let settings = super::settings(postgres.url())?;
    let first = Store::connect(&settings).await?;
    let second = Store::connect(&settings).await?;
    let (one, two) = tokio::join!(first.migrate(), second.migrate());
    one?;
    two?;
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(first.pool())
            .await?;
    assert_eq!(vec![1, 3, 12], versions);
    first.migrate().await?;
    let again: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
        .fetch_one(first.pool())
        .await?;
    assert_eq!(3, again);
    Ok(())
}

#[tokio::test]
async fn the_server_opens_and_migrates_the_store_at_start() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let text = format!("[state]\nurl = \"{}\"\n", postgres.url());
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let state = AppState::build(&settings).await?;
    let store = state.store().ok_or("the store is configured")?;
    let leases: i64 = sqlx::query_scalar("SELECT count(*) FROM lane_lease")
        .fetch_one(store.pool())
        .await?;
    assert_eq!(0, leases);
    store.ping().await?;
    Ok(())
}
