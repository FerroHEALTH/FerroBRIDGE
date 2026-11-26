// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The session settings every connection of the store starts with, read back
//! from a live session.

use std::time::Duration;

use omop_cdm::connection::session::Session;

use super::settings;

#[tokio::test]
async fn every_session_of_the_store_holds_its_timeouts() -> Result<(), Box<dyn std::error::Error>> {
    if !ferrobridge_testkit::containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = ferrobridge_testkit::containers::postgres().await?;
    let mut settings = settings(postgres.url())?;
    settings.connection = settings.connection.with_session(Session {
        statement_timeout: Duration::from_millis(1234),
        lock_timeout: Duration::from_millis(567),
        idle_in_transaction_session_timeout: Duration::from_secs(89),
        ..Session::default()
    })?;
    let store = ferrobridge_server::store::Store::open(&settings).await?;
    let mut shown = Vec::new();
    for statement in [
        "SHOW statement_timeout",
        "SHOW lock_timeout",
        "SHOW idle_in_transaction_session_timeout",
    ] {
        let value: String = sqlx::query_scalar(statement)
            .fetch_one(store.pool())
            .await?;
        shown.push(value);
    }
    assert_eq!(shown, ["1234ms", "567ms", "89s"]);
    Ok(())
}
