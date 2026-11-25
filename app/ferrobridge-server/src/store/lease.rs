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
    /// Returns the generation the store holds as `value`.
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

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

/// A lease this replica holds: the lease and the generation it was taken
/// at, the fence of every write made under it.
#[derive(Debug, Clone)]
pub struct Term {
    /// The lease.
    lease: Lease,
    /// The generation it was taken at.
    generation: Generation,
}

impl Term {
    /// Returns the term `lease` was taken at `generation` for.
    #[must_use]
    pub const fn new(lease: Lease, generation: Generation) -> Self {
        Self { lease, generation }
    }

    /// Returns the lease.
    #[must_use]
    pub const fn lease(&self) -> &Lease {
        &self.lease
    }

    /// Returns the generation.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }

    /// Returns the pool the lease statements run on, for a write fenced by
    /// this term.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.lease.pool
    }

    /// Refuses the caller's write on `connection` unless this term is still
    /// the lane's, as [`Lease::fence`] does.
    ///
    /// # Errors
    /// Returns every error of [`Lease::fence`].
    pub async fn fence(&self, connection: &mut PgConnection) -> Result<(), LeaseError> {
        self.lease.fence(connection, self.generation).await
    }
}

/// How a single-consumer lane takes, keeps and gives up its leadership.
///
/// [`Lease`] is the one production implementation; a test of a lane's loop
/// stands in its own.
#[async_trait::async_trait]
pub trait Leadership: fmt::Debug + Send + Sync {
    /// Takes the lane when no live holder has it; `None` while another
    /// replica holds it.
    ///
    /// # Errors
    /// Returns [`LeaseError`] when the claim cannot be made.
    async fn claim(&self) -> Result<Option<Term>, LeaseError>;

    /// Holds `term` for another period, unless another replica took it.
    ///
    /// # Errors
    /// Returns [`LeaseError`] when the renewal cannot be made.
    async fn renew(&self, term: &Term) -> Result<Renewed, LeaseError>;

    /// Gives `term` up, so another replica takes the lane at once.
    ///
    /// # Errors
    /// Returns [`LeaseError`] when the release cannot be made.
    async fn release(&self, term: &Term) -> Result<(), LeaseError>;

    /// Returns how often the holder renews.
    fn renew_every(&self) -> Duration;
}

#[async_trait::async_trait]
impl Leadership for Lease {
    async fn claim(&self) -> Result<Option<Term>, LeaseError> {
        Ok(Self::claim(self)
            .await?
            .map(|generation| Term::new(self.clone(), generation)))
    }

    async fn renew(&self, term: &Term) -> Result<Renewed, LeaseError> {
        Self::renew(self, term.generation).await
    }

    async fn release(&self, term: &Term) -> Result<(), LeaseError> {
        Self::release(self, term.generation).await
    }

    fn renew_every(&self) -> Duration {
        Self::renew_every(self)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Generation, Holder, Lane, Lease, LeaseError};

    #[test]
    fn a_lane_is_named() {
        assert!(matches!(Lane::new(""), Err(LeaseError::Lane)));
        let lane = Lane::new("outbound").expect("a named lane");
        assert_eq!("outbound", lane.as_str());
        assert_eq!("outbound", lane.to_string());
        assert_ne!(Holder::new(), Holder::default());
        assert_eq!(7, Generation(7).get());
    }

    #[tokio::test]
    async fn a_lease_renews_three_times_a_period_and_binds_its_period_in_milliseconds() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://bridge@db.invalid/state")
            .expect("a lazy pool");
        let lane = Lane::new("outbound").expect("a named lane");
        let lease = Lease::new(
            pool.clone(),
            lane.clone(),
            Holder::new(),
            Duration::from_secs(30),
        );
        assert_eq!(&lane, lease.lane());
        assert_eq!(Duration::from_secs(10), lease.renew_every());
        assert_eq!(30_000, lease.ttl_ms());
        let endless = Lease::new(pool, lane, Holder::new(), Duration::MAX);
        assert_eq!(i64::MAX, endless.ttl_ms());
    }

    #[test]
    fn a_stale_write_names_both_generations() {
        let stale = LeaseError::Stale {
            lane: Lane::new("outbound").expect("a named lane"),
            held: 3,
            current: Some(4),
        };
        assert_eq!(
            "lane outbound is at generation Some(4); the write held generation 3",
            stale.to_string()
        );
    }

    #[tokio::test]
    async fn an_unreachable_store_is_an_error_and_never_a_held_lease() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(Duration::from_millis(500))
            .connect_lazy("postgres://bridge@127.0.0.1:1/state?sslmode=disable")
            .expect("a lazy pool");
        let lease = Lease::new(
            pool,
            Lane::new("outbound").expect("a named lane"),
            Holder::new(),
            Duration::from_secs(30),
        );
        let held = Generation(1);
        assert!(matches!(lease.claim().await, Err(LeaseError::Database(_))));
        assert!(matches!(
            lease.renew(held).await,
            Err(LeaseError::Database(_))
        ));
        assert!(matches!(
            lease.release(held).await,
            Err(LeaseError::Database(_))
        ));
    }
}
