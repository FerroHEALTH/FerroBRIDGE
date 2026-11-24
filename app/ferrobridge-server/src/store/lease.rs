// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One leader per lane across replicas: a row lease in `lane_lease` with a
//! fencing generation.
//!
//! A replica takes a free or expired lease and the generation moves on, so a
//! former leader paused past its expiry holds a generation that is no longer
//! the lane's. Every cursor write runs [`Lease::fence`] in its own
//! transaction, which locks the lease row and refuses a generation that is
//! not current, so a takeover waits for the write and the former leader's
//! next write is refused. Session advisory locks carry no fencing token and
//! end with their connection while the task may run on
//! (<https://www.postgresql.org/docs/current/explicit-locking.html#ADVISORY-LOCKS>).
//! No specification governs leadership: our own design on family precedent.

use std::fmt;
use std::time::Duration;

use sqlx::PgConnection;
use sqlx::postgres::PgPool;
use uuid::Uuid;

/// The lane a lease names, such as a change-feed reader.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lane(String);

impl Lane {
    /// Returns the lane `name` names.
    ///
    /// # Errors
    /// Returns [`LeaseError::Lane`] for an empty name.
    pub fn new(name: impl Into<String>) -> Result<Self, LeaseError> {
        let name = name.into();
        if name.is_empty() {
            return Err(LeaseError::Lane);
        }
        Ok(Self(name))
    }

    /// Returns the name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Lane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A replica, as the holder of the leases it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Holder(Uuid);

impl Holder {
    /// Returns a holder id no other replica holds.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for Holder {
    fn default() -> Self {
        Self::new()
    }
}

/// The generation a lease was taken at: the fencing token every write under
/// the lease carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(i64);

impl Generation {
    /// Returns the number the store holds.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

/// Whether a renewal kept the lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum Renewed {
    /// The lease is held for another period.
    Held,
    /// Another replica took the lease; the holder stops its work.
    Lost,
}

/// Why a lease operation failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LeaseError {
    /// A lane name is empty.
    #[error("a lane name is not empty")]
    Lane,
    /// The database refused or failed a statement.
    #[error("the lease statement failed")]
    Database(#[from] sqlx::Error),
    /// A write carried a generation that is no longer the lane's.
    #[error("lane {lane} is at generation {current:?}; the write held generation {held}")]
    Stale {
        /// The lane.
        lane: Lane,
        /// The generation the write held.
        held: i64,
        /// The lane's generation, `None` when no replica ever took it.
        current: Option<i64>,
    },
}

/// One replica's lease on one lane.
#[derive(Debug, Clone)]
pub struct Lease {
    /// The pool the lease statements run on.
    pool: PgPool,
    /// The lane.
    lane: Lane,
    /// This replica.
    holder: Holder,
    /// How long a claim or a renewal holds the lease.
    ttl: Duration,
}

impl Lease {
    /// Returns `holder`'s lease on `lane`, held for `ttl` at a time.
    #[must_use]
    pub const fn new(pool: PgPool, lane: Lane, holder: Holder, ttl: Duration) -> Self {
        Self {
            pool,
            lane,
            holder,
            ttl,
        }
    }

    /// Returns the lane.
    #[must_use]
    pub const fn lane(&self) -> &Lane {
        &self.lane
    }

    /// Returns how often the holder renews: a third of the period, so two
    /// renewals may fail before the lease runs out.
    #[must_use]
    pub fn renew_every(&self) -> Duration {
        self.ttl / 3
    }

    /// The period in milliseconds, as the statements bind it.
    fn ttl_ms(&self) -> i64 {
        // NOTE: no specification governs this: our own design; a period past
        // i64 milliseconds is held as the longest the column takes.
        i64::try_from(self.ttl.as_millis()).unwrap_or(i64::MAX)
    }

    /// Takes the lease when no live holder has it, moving the lane to its
    /// next generation; `None` while another replica holds it.
    ///
    /// # Errors
    /// Returns [`LeaseError::Database`] when a statement fails.
    pub async fn claim(&self) -> Result<Option<Generation>, LeaseError> {
        sqlx::query("INSERT INTO lane_lease (lane) VALUES ($1) ON CONFLICT (lane) DO NOTHING")
            .bind(self.lane.as_str())
            .execute(&self.pool)
            .await?;
        let taken: Option<i64> = sqlx::query_scalar(
            "UPDATE lane_lease SET holder = $2, generation = generation + 1, \
             leased_until = clock_timestamp() + $3 * interval '1 millisecond' \
             WHERE lane = $1 AND (holder IS NULL OR holder = $2 OR leased_until < clock_timestamp()) \
             RETURNING generation",
        )
        .bind(self.lane.as_str())
        .bind(self.holder.0)
        .bind(self.ttl_ms())
        .fetch_optional(&self.pool)
        .await?;
        Ok(taken.map(Generation))
    }

    /// Holds the lease taken at `generation` for another period, unless
    /// another replica has taken it since.
    ///
    /// # Errors
    /// Returns [`LeaseError::Database`] when the statement fails.
    pub async fn renew(&self, generation: Generation) -> Result<Renewed, LeaseError> {
        let renewed = sqlx::query(
            "UPDATE lane_lease SET leased_until = clock_timestamp() + $4 * interval '1 millisecond' \
             WHERE lane = $1 AND holder = $2 AND generation = $3",
        )
        .bind(self.lane.as_str())
        .bind(self.holder.0)
        .bind(generation.0)
        .bind(self.ttl_ms())
        .execute(&self.pool)
        .await?;
        Ok(if renewed.rows_affected() == 1 {
            Renewed::Held
        } else {
            Renewed::Lost
        })
    }

    /// Gives up the lease taken at `generation`, so another replica takes
    /// the lane at once; a lease taken since is left alone.
    ///
    /// # Errors
    /// Returns [`LeaseError::Database`] when the statement fails.
    pub async fn release(&self, generation: Generation) -> Result<(), LeaseError> {
        sqlx::query(
            "UPDATE lane_lease SET holder = NULL, leased_until = NULL \
             WHERE lane = $1 AND holder = $2 AND generation = $3",
        )
        .bind(self.lane.as_str())
        .bind(self.holder.0)
        .bind(generation.0)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Refuses a write under `generation` unless it is the lane's current
    /// one, holding the lease row until the caller's transaction on
    /// `connection` ends, so no takeover lands between the check and the
    /// write.
    ///
    /// # Errors
    /// Returns [`LeaseError::Stale`] when another replica took the lane
    /// since, and [`LeaseError::Database`] when the statement fails.
    pub async fn fence(
        &self,
        connection: &mut PgConnection,
        generation: Generation,
    ) -> Result<(), LeaseError> {
        let current: Option<i64> =
            sqlx::query_scalar("SELECT generation FROM lane_lease WHERE lane = $1 FOR SHARE")
                .bind(self.lane.as_str())
                .fetch_optional(&mut *connection)
                .await?;
        if current == Some(generation.0) {
            return Ok(());
        }
        Err(LeaseError::Stale {
            lane: self.lane.clone(),
            held: generation.0,
            current,
        })
    }
}
