// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The sources' rows: the head row, every version, and the tenants and MLLP
//! routes derived from the head document, written in one transaction that
//! notifies `ferrobridge_sources` when it commits.
//!
//! A change and a deletion name the version they replace, and a write over a
//! version another writer replaced since is refused, the precondition of RFC
//! 9110 §13.1.1. No specification governs a bridge's source registry: our
//! own design.

use ferrobridge_operator_api::sources::{Kind, SourceVersion, Write};
use sqlx::PgConnection;
use sqlx::postgres::PgPool;
use uuid::Uuid;

use super::Source;
use super::document::{Document, Invalid};

/// The channel every replica hears a change of a source on.
pub const CHANNEL: &str = "ferrobridge_sources";

/// The columns a source is read with.
const COLUMNS: &str = "id, name, kind, enabled, version, document::text, updated_by, \
     to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')";

/// One source row as it is read.
type Row = (Uuid, String, String, bool, i64, String, String, String);

/// One version row as it is read.
type VersionRow = (i64, String, bool, Option<String>, bool, String, String);

/// Why a write of a source was refused or failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WriteError {
    /// The document breaks a rule of its kind, or names a secret the
    /// `[secrets]` directory does not hold.
    #[error("the source is refused: {0}")]
    Invalid(#[from] Invalid),
    /// Another source carries the name.
    #[error("another source is named so")]
    Exists,
    /// Another source routes one of the MLLP senders.
    #[error("another source routes one of these MLLP senders")]
    RouteTaken,
    /// No source has the id.
    #[error("no source has this id")]
    NotFound,
    /// The source is at another version than the write named.
    #[error("the source is at version {current}")]
    Stale {
        /// The source's version.
        current: i64,
    },
    /// A change names another kind than the source's.
    #[error("a change keeps the source's kind")]
    KindChanged,
    /// The store refused or failed a statement.
    #[error("the source store failed")]
    Store(#[source] sqlx::Error),
}

impl From<sqlx::Error> for WriteError {
    fn from(error: sqlx::Error) -> Self {
        match error
            .as_database_error()
            .and_then(|database| database.constraint())
        {
            Some("inbound_source_name_key") => Self::Exists,
            Some("inbound_route_mllp_pkey") => Self::RouteTaken,
            _ => Self::Store(error),
        }
    }
}

/// The sources' rows in the state store behind one pool.
#[derive(Debug, Clone)]
pub struct SourceStore {
    /// The state store's pool.
    pool: PgPool,
}

impl SourceStore {
    /// Returns the rows the state store behind `pool` keeps.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns every source, by name.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the read fails, or a row's document does
    /// not read.
    pub async fn list(&self) -> Result<Vec<Source>, sqlx::Error> {
        let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM inbound_source ORDER BY name"
        )))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(source).collect()
    }

    /// Returns the source `id`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the read fails, or its document does not
    /// read.
    pub async fn get(&self, id: Uuid) -> Result<Option<Source>, sqlx::Error> {
        let mut connection = self.pool.acquire().await?;
        read(&mut connection, id).await
    }

    /// Returns the source named `name`.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the read fails, or its document does not
    /// read.
    pub async fn named(&self, name: &str) -> Result<Option<Source>, sqlx::Error> {
        let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM inbound_source WHERE name = $1"
        )))
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        row.map(source).transpose()
    }

    /// Returns the source the MLLP sender `application` at `facility` is
    /// routed to.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the read fails, or its document does not
    /// read.
    pub async fn routed(
        &self,
        application: &str,
        facility: &str,
    ) -> Result<Option<Source>, sqlx::Error> {
        let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM inbound_source WHERE id = (SELECT source_id FROM inbound_route_mllp \
             WHERE sending_application = $1 AND sending_facility = $2)"
        )))
        .bind(application)
        .bind(facility)
        .fetch_optional(&self.pool)
        .await?;
        row.map(source).transpose()
    }

    /// Returns every version of the source `id`, oldest first; none for an
    /// id no source ever had.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the read fails.
    pub async fn versions(&self, id: Uuid) -> Result<Vec<SourceVersion>, sqlx::Error> {
        let rows: Vec<VersionRow> = sqlx::query_as(
            "SELECT version, name, enabled, document::text, deleted, committer, \
             to_char(committed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') \
             FROM inbound_source_version WHERE source_id = $1 ORDER BY version",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(
                |(version, name, enabled, document, deleted, committer, committed_at)| {
                    Ok(SourceVersion {
                        version: version.cast_unsigned(),
                        name,
                        enabled,
                        document: document
                            .map(|text| serde_json::from_str(&text))
                            .transpose()
                            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
                        deleted,
                        committer,
                        committed_at,
                    })
                },
            )
            .collect()
    }

    /// Creates the source `write` describes, its document read as
    /// `document`, as version 1 committed by `committer`.
    ///
    /// # Errors
    /// Returns [`WriteError::Exists`] or [`WriteError::RouteTaken`] for a
    /// name or a route another source holds, and [`WriteError::Store`] when
    /// a statement fails.
    pub async fn create(
        &self,
        write: &Write,
        document: &Document,
        committer: &str,
    ) -> Result<Source, WriteError> {
        let mut tx = self.pool.begin().await?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO inbound_source (name, kind, enabled, version, document, updated_by) \
             VALUES ($1, $2, $3, 1, $4::jsonb, $5) RETURNING id",
        )
        .bind(&write.name)
        .bind(write.kind.as_str())
        .bind(write.enabled)
        .bind(write.document.to_string())
        .bind(committer)
        .fetch_one(&mut *tx)
        .await?;
        record(&mut tx, id, 1, write, document, committer).await?;
        let created = read(&mut tx, id).await?.ok_or(WriteError::NotFound)?;
        tx.commit().await?;
        Ok(created)
    }

    /// Changes the source `id`, at version `expected` when one is named, to
    /// what `write` describes, committed by `committer`.
    ///
    /// # Errors
    /// Returns [`WriteError::NotFound`], [`WriteError::Stale`],
    /// [`WriteError::KindChanged`], [`WriteError::Exists`] and
    /// [`WriteError::RouteTaken`] for a write the store refuses, and
    /// [`WriteError::Store`] when a statement fails.
    pub async fn update(
        &self,
        id: Uuid,
        expected: Option<i64>,
        write: &Write,
        document: &Document,
        committer: &str,
    ) -> Result<Source, WriteError> {
        let mut tx = self.pool.begin().await?;
        let (kind, current) = locked(&mut tx, id).await?;
        if expected.is_some_and(|expected| expected != current) {
            return Err(WriteError::Stale { current });
        }
        if kind != write.kind.as_str() {
            return Err(WriteError::KindChanged);
        }
        let version = current.saturating_add(1);
        sqlx::query(
            "UPDATE inbound_source SET name = $2, enabled = $3, version = $4, document = $5::jsonb, \
             updated_by = $6, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(&write.name)
        .bind(write.enabled)
        .bind(version)
        .bind(write.document.to_string())
        .bind(committer)
        .execute(&mut *tx)
        .await?;
        for table in ["inbound_tenant", "inbound_route_mllp"] {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DELETE FROM {table} WHERE source_id = $1"
            )))
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        record(&mut tx, id, version, write, document, committer).await?;
        let changed = read(&mut tx, id).await?.ok_or(WriteError::NotFound)?;
        tx.commit().await?;
        Ok(changed)
    }

    /// Deletes the source `id`, at version `expected` when one is named,
    /// recording the deletion as its last version committed by `committer`.
    ///
    /// # Errors
    /// Returns [`WriteError::NotFound`] and [`WriteError::Stale`] for a
    /// write the store refuses, and [`WriteError::Store`] when a statement
    /// fails.
    pub async fn delete(
        &self,
        id: Uuid,
        expected: Option<i64>,
        committer: &str,
    ) -> Result<(), WriteError> {
        let mut tx = self.pool.begin().await?;
        let (_kind, current) = locked(&mut tx, id).await?;
        if expected.is_some_and(|expected| expected != current) {
            return Err(WriteError::Stale { current });
        }
        let version = current.saturating_add(1);
        sqlx::query(
            "INSERT INTO inbound_source_version (source_id, version, name, kind, enabled, document, deleted, committer) \
             SELECT id, $2, name, kind, false, NULL, true, $3 FROM inbound_source WHERE id = $1",
        )
        .bind(id)
        .bind(version)
        .bind(committer)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM inbound_source WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        notify(&mut tx, id, "deleted").await?;
        tx.commit().await?;
        Ok(())
    }
}

/// Locks the head row of `id` and returns its kind and version.
async fn locked(connection: &mut PgConnection, id: Uuid) -> Result<(String, i64), WriteError> {
    sqlx::query_as("SELECT kind, version FROM inbound_source WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(connection)
        .await?
        .ok_or(WriteError::NotFound)
}

/// Reads the source `id` inside the caller's transaction.
async fn read(connection: &mut PgConnection, id: Uuid) -> Result<Option<Source>, sqlx::Error> {
    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM inbound_source WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(connection)
    .await?;
    row.map(source).transpose()
}

/// Appends `version` of `id`, writes the tenants and routes its document
/// declares, and notifies every replica, inside the caller's transaction.
async fn record(
    connection: &mut PgConnection,
    id: Uuid,
    version: i64,
    write: &Write,
    document: &Document,
    committer: &str,
) -> Result<(), WriteError> {
    sqlx::query(
        "INSERT INTO inbound_source_version (source_id, version, name, kind, enabled, document, committer) \
         VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7)",
    )
    .bind(id)
    .bind(version)
    .bind(&write.name)
    .bind(write.kind.as_str())
    .bind(write.enabled)
    .bind(write.document.to_string())
    .bind(committer)
    .execute(&mut *connection)
    .await?;
    for tenant in document.tenants() {
        sqlx::query(
            "INSERT INTO inbound_tenant (source_id, tenant, identifier_system, credentials_ref, enabled) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(&tenant.name)
        .bind(&tenant.identifier_system)
        .bind(tenant.credentials.as_ref().map(|secret| secret.secret.as_str()))
        .bind(tenant.enabled)
        .execute(&mut *connection)
        .await?;
    }
    for route in document.routes() {
        sqlx::query(
            "INSERT INTO inbound_route_mllp (sending_application, sending_facility, source_id, \
             certificate_subject, source_cidrs) VALUES ($1, $2, $3, $4, $5::cidr[])",
        )
        .bind(&route.sending_application)
        .bind(&route.sending_facility)
        .bind(id)
        .bind(route.certificate_subject())
        .bind(
            route
                .networks()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        )
        .execute(&mut *connection)
        .await?;
    }
    notify(connection, id, &version.to_string()).await?;
    Ok(())
}

/// Notifies every replica that `id` moved to `version`, delivered when the
/// caller's transaction commits.
async fn notify(connection: &mut PgConnection, id: Uuid, version: &str) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(CHANNEL)
        .bind(format!("{id}:{version}"))
        .execute(connection)
        .await?;
    Ok(())
}

/// Returns the source `row` holds.
fn source(row: Row) -> Result<Source, sqlx::Error> {
    let (id, name, kind, enabled, version, document, updated_by, updated_at) = row;
    let kind = match kind.as_str() {
        "webhook" => Kind::Webhook,
        "fhir-subscription" => Kind::FhirSubscription,
        "mllp" => Kind::Mllp,
        other => {
            return Err(sqlx::Error::Decode(
                format!("the source kind {other} is unknown").into(),
            ));
        }
    };
    let value: serde_json::Value =
        serde_json::from_str(&document).map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    let parsed =
        Document::read(kind, &value).map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    Ok(Source {
        id,
        name,
        kind,
        enabled,
        version,
        document: value,
        parsed,
        updated_by,
        updated_at,
    })
}
