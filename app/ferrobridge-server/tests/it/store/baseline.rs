// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The baseline revision of the mapping set is recorded once, whichever
//! replica starts first on an empty store.

use std::error::Error;

use ferrobridge_operator_api::revisions::Origin;
use ferrobridge_server::mappings::revisions::Revisions;
use ferrobridge_testkit::containers;

#[tokio::test]
async fn replicas_starting_together_on_an_empty_store_record_one_baseline()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let first = super::open(postgres.url()).await?;
    let second = super::open(postgres.url()).await?;
    let (left, right) = (Revisions::new(&first), Revisions::new(&second));
    let (one, two) = tokio::join!(left.baseline(), right.baseline());
    let recorded: Vec<_> = [one?, two?].into_iter().flatten().collect();
    let [baseline] = recorded.as_slice() else {
        return Err(format!("{} baselines were recorded", recorded.len()).into());
    };
    assert_eq!(Origin::Baseline, baseline.origin);
    assert!(baseline.files.is_empty(), "{:?}", baseline.files);
    let (running, revisions) = left.list().await?;
    assert_eq!(Some(baseline.id.clone()), running);
    assert_eq!(1, revisions.len());
    assert!(right.baseline().await?.is_none());
    Ok(())
}
