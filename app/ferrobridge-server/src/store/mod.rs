// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PostgreSQL state store, which holds the bridge's own state so every
//! replica answers from one copy (`docs/architecture.md` §7, "The state
//! store").
//!
//! The migrations under `migrations/state/` ship in the binary, one baseline
//! file per area, and run at start; `sqlx` takes a PostgreSQL advisory lock
//! around them, so replicas starting together migrate once. The OMOP CDM is
//! a database of its own and never this one. No specification governs where
//! the bridge keeps its state: our own design on family precedent.

pub mod lease;
pub mod listen;
pub mod seal;

use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

use lease::{Holder, Lane, Lease};
use listen::{Listened, Listeners};
use seal::SealKeys;

/// The oldest PostgreSQL the store runs on, as a `server_version_num`.
///
/// The setting is described in PostgreSQL 19 docs, "Preset Options". No
/// specification governs the minimum: our own design, the release every
/// product of the family pins.
pub const MINIMUM_SERVER_VERSION: i32 = 190_000;

/// The `application_name` of every pooled connection, which tells an
/// operator the bridge's sessions apart in `pg_stat_activity`.
pub const APPLICATION_NAME: &str = "ferrobridge state";

/// The `[state]` section, resolved.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The connection, with its TLS settled as the CDM's is.
    pub connection: omop_cdm::connection::CdmConnection,
    /// The most connections the pool holds.
    pub max_connections: u32,
    /// How long a request waits for a connection.
    pub acquire_timeout: Duration,
    /// The sealing keys, when `[state.seal]` is present.
    pub seal: Option<SealKeys>,
}

/// Why the store failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// The connection's TLS relay could not be started.
    #[error("the state database connection could not be prepared")]
    Connection(#[from] omop_cdm::connection::ConnectionError),
    /// The database refused or failed a statement, or cannot be reached.
    #[error("the state database failed")]
    Database(#[from] sqlx::Error),
    /// A migration failed.
    #[error("the state database could not be migrated")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// The database runs a PostgreSQL older than the store supports.
    #[error(
        "the state database runs PostgreSQL server_version_num {found}, and FerroBRIDGE needs {required} or newer"
    )]
    UnsupportedServer {
        /// The `server_version_num` the database reported.
        found: i32,
        /// [`MINIMUM_SERVER_VERSION`].
        required: i32,
    },
}

/// Refuses a `server_version_num` older than [`MINIMUM_SERVER_VERSION`].
///
/// # Errors
/// Returns [`StoreError::UnsupportedServer`] naming the version found and
/// the version required.
pub const fn supported(found: i32) -> Result<(), StoreError> {
    if found < MINIMUM_SERVER_VERSION {
        return Err(StoreError::UnsupportedServer {
            found,
            required: MINIMUM_SERVER_VERSION,
        });
    }
    Ok(())
}

/// The store: a pool of connections to one database, and this replica's
/// holder id for the lanes it leases.
#[derive(Debug, Clone)]
pub struct Store {
    /// The pool.
    pool: PgPool,
    /// The options the pool connects with, for the `LISTEN` session.
    options: PgConnectOptions,
    /// This replica, as the holder of every lease it takes.
    holder: Holder,
    /// The connection, held so its TLS relay runs as long as the store.
    connection: omop_cdm::connection::CdmConnection,
    /// The sealing keys, when `[state.seal]` is present.
    seal: Option<SealKeys>,
}

impl Store {
    /// Connects as `settings` describe and refuses a server older than
    /// [`MINIMUM_SERVER_VERSION`].
    ///
    /// # Errors
    /// Returns [`StoreError::Connection`] when the TLS relay cannot start,
    /// [`StoreError::Database`] when the database cannot be reached and
    /// [`StoreError::UnsupportedServer`] when it runs an older PostgreSQL.
    pub async fn connect(settings: &Settings) -> Result<Self, StoreError> {
        let options = settings
            .connection
            .pool_options()?
            .application_name(APPLICATION_NAME);
        let pool = PgPoolOptions::new()
            .max_connections(settings.max_connections)
            .acquire_timeout(settings.acquire_timeout)
            .connect_with(options.clone())
            .await?;
        let found: i32 = sqlx::query_scalar("SELECT current_setting('server_version_num')::int")
            .fetch_one(&pool)
            .await?;
        supported(found)?;
        Ok(Self {
            pool,
            options,
            holder: Holder::new(),
            connection: settings.connection.clone(),
            seal: settings.seal.clone(),
        })
    }

    /// Connects as [`Self::connect`] does, then runs the migrations.
    ///
    /// # Errors
    /// Returns every error of [`Self::connect`] and [`Self::migrate`].
    pub async fn open(settings: &Settings) -> Result<Self, StoreError> {
        let store = Self::connect(settings).await?;
        store.migrate().await?;
        Ok(store)
    }

    /// Runs the migrations the binary carries, under `sqlx`'s advisory lock.
    ///
    /// # Errors
    /// Returns [`StoreError::Migrate`] when a migration fails.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        sqlx::migrate!("./migrations/state").run(&self.pool).await?;
        Ok(())
    }

    /// Returns whether the database answers a statement, for readiness.
    ///
    /// # Errors
    /// Returns [`StoreError::Database`] when it does not.
    pub async fn ping(&self) -> Result<(), StoreError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    /// Returns the pool, for an area's own statements.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Returns this replica's holder id.
    #[must_use]
    pub const fn holder(&self) -> Holder {
        self.holder
    }

    /// Returns the lease of `lane` this replica takes, held for `ttl` at a
    /// time.
    #[must_use]
    pub fn lease(&self, lane: Lane, ttl: Duration) -> Lease {
        Lease::new(self.pool.clone(), lane, self.holder, ttl)
    }

    /// Returns one `LISTEN` session, apart from the pool, that carries the
    /// channel of every copy in `listened`.
    #[must_use]
    pub fn listeners(&self, listened: &[Arc<Listened>]) -> Listeners {
        Listeners::on(self.options.clone(), listened)
    }

    /// Returns the sealing keys, when `[state.seal]` is present.
    #[must_use]
    pub const fn seal(&self) -> Option<&SealKeys> {
        self.seal.as_ref()
    }

    /// Returns the connection the store was opened on.
    #[must_use]
    pub fn connection(&self) -> &omop_cdm::connection::CdmConnection {
        &self.connection
    }
}

#[cfg(test)]
mod tests {
    use super::{MINIMUM_SERVER_VERSION, StoreError, supported};

    #[test]
    fn a_server_older_than_postgresql_19_is_refused() {
        assert!(supported(MINIMUM_SERVER_VERSION).is_ok());
        assert!(supported(190_002).is_ok());
        assert!(matches!(
            supported(180_006),
            Err(StoreError::UnsupportedServer {
                found: 180_006,
                required: 190_000
            })
        ));
    }
}
