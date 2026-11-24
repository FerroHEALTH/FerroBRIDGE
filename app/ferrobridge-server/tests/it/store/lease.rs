// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One leader per lane: a takeover once the lease expires, and a cursor
//! write under the former leader's generation refused.

use std::error::Error;
use std::time::Duration;

use ferrobridge_server::store::lease::{Generation, Lane, Lease, LeaseError, Renewed};
use ferrobridge_testkit::containers;
use sqlx::PgPool;

/// The period every lease of these cases is held for.
const TTL: Duration = Duration::from_millis(300);

/// Moves the cursor of `lease`'s lane to `position` under `generation`, in
/// one transaction fenced by the lease.
async fn advance(
    pool: &PgPool,
    lease: &Lease,
    generation: Generation,
    position: i64,
) -> Result<(), LeaseError> {
    let mut tx = pool.begin().await?;
    lease.fence(&mut tx, generation).await?;
    sqlx::query("UPDATE test_cursor SET position = $2 WHERE lane = $1")
        .bind(lease.lane().as_str())
        .bind(position)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[tokio::test]
async fn a_lease_is_taken_over_after_expiry_and_the_stale_generation_is_refused()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let first = super::open(postgres.url()).await?;
    let second = super::open(postgres.url()).await?;
    let lane = Lane::new("change-feed:test")?;
    let leader = first.lease(lane.clone(), TTL);
    let follower = second.lease(lane.clone(), TTL);
    sqlx::query("CREATE TABLE test_cursor (lane text PRIMARY KEY, position bigint NOT NULL)")
        .execute(first.pool())
        .await?;
    sqlx::query("INSERT INTO test_cursor VALUES ($1, 0)")
        .bind(lane.as_str())
        .execute(first.pool())
        .await?;

    let old = leader.claim().await?.ok_or("the lane is free")?;
    assert_eq!(None, follower.claim().await?);
    assert_eq!(Renewed::Held, leader.renew(old).await?);
    advance(first.pool(), &leader, old, 5).await?;

    tokio::time::sleep(TTL * 2).await;
    let new = follower.claim().await?.ok_or("the lease has expired")?;
    assert!(new > old, "{new:?} after {old:?}");
    assert_eq!(Renewed::Lost, leader.renew(old).await?);
    assert_eq!(None, leader.claim().await?);

    let refused = advance(first.pool(), &leader, old, 9).await;
    assert!(
        matches!(
            &refused,
            Err(LeaseError::Stale { held, current: Some(current), .. })
                if *held == old.get() && *current == new.get()
        ),
        "{refused:?}"
    );
    advance(second.pool(), &follower, new, 7).await?;
    let position: i64 = sqlx::query_scalar("SELECT position FROM test_cursor WHERE lane = $1")
        .bind(lane.as_str())
        .fetch_one(first.pool())
        .await?;
    assert_eq!(7, position);
    Ok(())
}

#[tokio::test]
async fn a_released_lease_is_taken_at_once() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let first = super::open(postgres.url()).await?;
    let second = super::open(postgres.url()).await?;
    let lane = Lane::new("outbound")?;
    let leader = first.lease(lane.clone(), Duration::from_secs(60));
    let follower = second.lease(lane, Duration::from_secs(60));
    let held = leader.claim().await?.ok_or("the lane is free")?;
    assert_eq!(None, follower.claim().await?);
    leader.release(held).await?;
    let taken = follower.claim().await?.ok_or("the lease was released")?;
    assert!(taken > held);
    Ok(())
}
