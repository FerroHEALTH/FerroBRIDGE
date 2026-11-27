// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The statements over the record-once rows, each on a connection the caller
//! holds, so one entry's writes can share a transaction.
//!
//! Every write inserts with `ON CONFLICT DO NOTHING` and answers the row
//! that stands, read again in its own statement only when a concurrent
//! writer's row was not yet visible to it. No specification governs the
//! storage: our own design.

use sqlx::PgConnection;
use sqlx::Row as _;

use crate::identity::ExternalResourceId;
use crate::identity::FhirResourceId;
use crate::identity::record::CommittedSource;
use crate::identity::record::CompositionBinding;
use crate::identity::record::ConsumedSource;
use crate::identity::record::SourceVersion;
use crate::identity::store::EntryWrite;
use crate::identity::store::StoreError;

/// Returns the identifier `value` read under `key` as a `kind`.
pub(super) fn checked<T, E: std::error::Error + Send + Sync + 'static>(
    key: &str,
    kind: &'static str,
    value: String,
    parse: impl FnOnce(&str) -> Result<T, E>,
) -> Result<T, StoreError> {
    parse(&value).map_err(|source| StoreError::Identifier {
        key: key.to_owned(),
        kind,
        value,
        source: Box::new(source),
    })
}

/// Returns the internal id recorded for `external`, when one is.
pub(super) async fn internal(
    connection: &mut PgConnection,
    resource_type: &str,
    external: &ExternalResourceId,
) -> Result<Option<FhirResourceId>, StoreError> {
    let found: Option<String> = sqlx::query_scalar(
        "SELECT internal_id FROM external_internal WHERE resource_type = $1 AND external_id = $2",
    )
    .bind(resource_type)
    .bind(external.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    found
        .map(|value| checked(external.as_str(), "FHIR id", value, FhirResourceId::new))
        .transpose()
}

/// Records `internal` as the id of `external`, and returns the one that
/// stands.
pub(super) async fn record_internal(
    connection: &mut PgConnection,
    resource_type: &str,
    external: &ExternalResourceId,
    internal_id: &FhirResourceId,
) -> Result<FhirResourceId, StoreError> {
    let stood: Option<String> = sqlx::query_scalar(
        "WITH won AS (INSERT INTO external_internal (resource_type, external_id, internal_id) \
           VALUES ($1, $2, $3) ON CONFLICT (resource_type, external_id) DO NOTHING \
           RETURNING internal_id) \
         SELECT internal_id FROM won UNION ALL SELECT internal_id FROM external_internal \
         WHERE resource_type = $1 AND external_id = $2 LIMIT 1",
    )
    .bind(resource_type)
    .bind(external.as_str())
    .bind(internal_id.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    let stood = match stood {
        Some(value) => Some(checked(
            external.as_str(),
            "FHIR id",
            value,
            FhirResourceId::new,
        )?),
        None => internal(connection, resource_type, external).await?,
    };
    stood.ok_or_else(|| StoreError::Missing {
        key: external.as_str().to_owned(),
    })
}

/// Returns the binding a row holds.
fn binding_of_row(key: &str, value: serde_json::Value) -> Result<CompositionBinding, StoreError> {
    serde_json::from_value(value).map_err(|source| StoreError::Record {
        key: key.to_owned(),
        source: Box::new(source),
    })
}

/// Returns where `internal` lives, when the map knows.
pub(super) async fn binding(
    connection: &mut PgConnection,
    resource_type: &str,
    internal_id: &FhirResourceId,
) -> Result<Option<CompositionBinding>, StoreError> {
    let found: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT binding FROM composition_binding WHERE resource_type = $1 AND internal_id = $2",
    )
    .bind(resource_type)
    .bind(internal_id.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    found
        .map(|value| binding_of_row(internal_id.as_str(), value))
        .transpose()
}

/// Records where `internal` lives, and returns the binding that stands.
pub(super) async fn record_binding(
    connection: &mut PgConnection,
    resource_type: &str,
    internal_id: &FhirResourceId,
    offered: &CompositionBinding,
) -> Result<CompositionBinding, StoreError> {
    let value = serde_json::to_value(offered).map_err(|source| StoreError::Record {
        key: internal_id.as_str().to_owned(),
        source: Box::new(source),
    })?;
    let stood: Option<serde_json::Value> = sqlx::query_scalar(
        "WITH won AS (INSERT INTO composition_binding (resource_type, internal_id, binding) \
           VALUES ($1, $2, $3) ON CONFLICT (resource_type, internal_id) DO NOTHING \
           RETURNING binding) \
         SELECT binding FROM won UNION ALL SELECT binding FROM composition_binding \
         WHERE resource_type = $1 AND internal_id = $2 LIMIT 1",
    )
    .bind(resource_type)
    .bind(internal_id.as_str())
    .bind(&value)
    .fetch_optional(&mut *connection)
    .await?;
    let stood = match stood {
        Some(value) => Some(binding_of_row(internal_id.as_str(), value)?),
        None => binding(connection, resource_type, internal_id).await?,
    };
    stood.ok_or_else(|| StoreError::Missing {
        key: internal_id.as_str().to_owned(),
    })
}

/// Returns what consumed `source`, when the map knows.
pub(super) async fn consumed(
    connection: &mut PgConnection,
    source: &SourceVersion,
) -> Result<Option<ConsumedSource>, StoreError> {
    let row = keyed(
        "SELECT ehr_id, versioned_object_uid, internal_id, context FROM consumed_source \
         WHERE resource_type = $1 AND source_id = $2 AND version_id = $3",
        source,
    )
    .fetch_optional(&mut *connection)
    .await?;
    row.map(|row| consumed_of_row(&row)).transpose()
}

/// Records what consumed `source`, and returns the record that stands.
pub(super) async fn record_consumed(
    connection: &mut PgConnection,
    source: &SourceVersion,
    offered: &ConsumedSource,
) -> Result<ConsumedSource, StoreError> {
    let stood = keyed(
        "WITH won AS (INSERT INTO consumed_source (resource_type, source_id, version_id, \
           ehr_id, versioned_object_uid, internal_id, context) \
           VALUES ($1, $2, $3, $4, $5, $6, $7) \
           ON CONFLICT (resource_type, source_id, version_id) DO NOTHING \
           RETURNING ehr_id, versioned_object_uid, internal_id, context) \
         SELECT ehr_id, versioned_object_uid, internal_id, context FROM won UNION ALL \
         SELECT ehr_id, versioned_object_uid, internal_id, context FROM consumed_source \
         WHERE resource_type = $1 AND source_id = $2 AND version_id = $3 LIMIT 1",
        source,
    )
    .bind(&offered.ehr_id)
    .bind(&offered.versioned_object_uid)
    .bind(&offered.internal_id)
    .bind(&offered.context)
    .fetch_optional(&mut *connection)
    .await?;
    let stood = match stood {
        Some(row) => Some(consumed_of_row(&row)?),
        None => consumed(connection, source).await?,
    };
    stood.ok_or_else(|| StoreError::Missing {
        key: source.storage_key(),
    })
}

/// Records that the resource `internal` carries the identifier `system` and
/// `value_hmac`, its value as a digest.
pub(super) async fn record_identifier(
    connection: &mut PgConnection,
    resource_type: &str,
    (system, value_hmac): (&str, &[u8]),
    internal_id: &FhirResourceId,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO resource_identifier (resource_type, system, value_hmac, internal_id) \
         VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
    )
    .bind(resource_type)
    .bind(system)
    .bind(value_hmac)
    .bind(internal_id.as_str())
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Records one committed entry in one statement: its internal id, its
/// binding, the identifiers `systems` and `digests` name pairwise, and the
/// consumed `source`; returns the id and the binding that stand.
///
/// `None` when a row the statement conflicted with was written by a
/// transaction that committed after the statement began, so the caller
/// records the entry again statement by statement.
pub(super) async fn record_entry(
    connection: &mut PgConnection,
    write: &EntryWrite<'_>,
    (systems, digests): (&[&str], &[Vec<u8>]),
) -> Result<Option<(FhirResourceId, CompositionBinding)>, StoreError> {
    let offered = serde_json::to_value(write.binding).map_err(|source| StoreError::Record {
        key: write.derived.as_str().to_owned(),
        source: Box::new(source),
    })?;
    let row = sqlx::query(
        "WITH won_internal AS (INSERT INTO external_internal (resource_type, external_id, \
           internal_id) VALUES ($1, $2, $3) ON CONFLICT (resource_type, external_id) DO NOTHING \
           RETURNING internal_id), \
         internal AS (SELECT internal_id FROM won_internal UNION ALL SELECT internal_id \
           FROM external_internal WHERE resource_type = $1 AND external_id = $2 LIMIT 1), \
         won_binding AS (INSERT INTO composition_binding (resource_type, internal_id, binding) \
           SELECT $1, internal_id, $4 FROM internal \
           ON CONFLICT (resource_type, internal_id) DO NOTHING RETURNING binding), \
         stood AS (SELECT binding FROM won_binding UNION ALL SELECT b.binding \
           FROM composition_binding b JOIN internal i ON b.internal_id = i.internal_id \
           WHERE b.resource_type = $1 LIMIT 1), \
         identifiers AS (INSERT INTO resource_identifier (resource_type, system, value_hmac, \
           internal_id) SELECT $1, u.system, u.value_hmac, i.internal_id FROM internal i, \
           UNNEST($5::text[], $6::bytea[]) AS u(system, value_hmac) ON CONFLICT DO NOTHING), \
         consumed AS (INSERT INTO consumed_source (resource_type, source_id, version_id, \
           ehr_id, versioned_object_uid, internal_id, context) \
           SELECT $7, $8, $9, s.binding ->> 'ehr_id', s.binding ->> 'versioned_object_uid', \
           i.internal_id, s.binding ->> 'context' FROM internal i, stood s \
           WHERE $7::text IS NOT NULL \
           ON CONFLICT (resource_type, source_id, version_id) DO NOTHING) \
         SELECT i.internal_id, s.binding FROM internal i JOIN stood s ON true",
    )
    .bind(write.resource_type)
    .bind(write.key.as_str())
    .bind(write.derived.as_str())
    .bind(&offered)
    .bind(systems)
    .bind(digests)
    .bind(write.source.map(SourceVersion::resource_type))
    .bind(write.source.map(|source| source.id().as_str()))
    .bind(write.source.map(version_slot))
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let id: String = row.try_get("internal_id")?;
    let id = checked(write.key.as_str(), "FHIR id", id, FhirResourceId::new)?;
    let stood = binding_of_row(id.as_str(), row.try_get("binding")?)?;
    Ok(Some((id, stood)))
}

/// Records one committed entry statement by statement in one transaction,
/// each statement reading the rows committed before it, and returns the id
/// and the binding that stand; `digests` pairs with `write.identifiers`.
pub(super) async fn record_entry_stepwise(
    connection: &mut PgConnection,
    write: &EntryWrite<'_>,
    digests: &[Vec<u8>],
) -> Result<(FhirResourceId, CompositionBinding), StoreError> {
    let mut transaction = sqlx::Connection::begin(&mut *connection).await?;
    let id = record_internal(
        &mut transaction,
        write.resource_type,
        write.key,
        write.derived,
    )
    .await?;
    let stood = record_binding(&mut transaction, write.resource_type, &id, write.binding).await?;
    for (identifier, digest) in write.identifiers.iter().zip(digests) {
        record_identifier(
            &mut transaction,
            write.resource_type,
            (identifier.system().unwrap_or_default(), digest),
            &id,
        )
        .await?;
    }
    if let Some(source) = write.source {
        record_consumed(&mut transaction, source, &ConsumedSource::of(&stood, &id)).await?;
    }
    transaction.commit().await?;
    Ok((id, stood))
}

/// Returns the statement `sql` with the three keys of `source` bound as its
/// first three parameters.
pub(super) fn keyed<'q>(
    sql: &'static str,
    source: &'q SourceVersion,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    sqlx::query(sql)
        .bind(source.resource_type())
        .bind(source.id().as_str())
        .bind(version_slot(source))
}

/// Returns the version slot of `source`: its `meta.versionId`, or the empty
/// string for none, which sorts before every version as the old keys did.
pub(super) fn version_slot(source: &SourceVersion) -> &str {
    source.version_id().unwrap_or_default()
}

/// Returns the committed record a row holds.
pub(super) fn committed_of_row(row: &sqlx::postgres::PgRow) -> Result<CommittedSource, StoreError> {
    Ok(CommittedSource {
        ehr_id: row.try_get("ehr_id")?,
        contribution_uid: row.try_get("contribution_uid")?,
    })
}

/// Returns the consumed record a row holds.
pub(super) fn consumed_of_row(row: &sqlx::postgres::PgRow) -> Result<ConsumedSource, StoreError> {
    Ok(ConsumedSource {
        ehr_id: row.try_get("ehr_id")?,
        versioned_object_uid: row.try_get("versioned_object_uid")?,
        internal_id: row.try_get("internal_id")?,
        context: row.try_get("context")?,
    })
}
