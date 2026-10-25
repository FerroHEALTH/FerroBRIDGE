// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR resource a composition built in memory was mapped from, kept in
//! the bridge schema beside the natural keys of its rows.
//!
//! On the FHIR to OMOP path a composition exists only in memory, so its
//! versioned object id names no CDR record; the `resource_origin` table ties
//! it to the resource's type, id and version, and through the `record` table
//! every row it wrote can be traced to the resource and back. The CDM's
//! `*_source_value` columns carry source codes and are not the place for an
//! identifier. No specification governs lineage storage: our own design.

use tokio_postgres::Transaction;

use crate::ddl::SchemaName;
use crate::writer::CdmWriter;
use crate::writer::database;
use crate::writer::error::Step;
use crate::writer::error::WriteError;

/// The FHIR resource a composition was mapped from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceOrigin {
    /// The resource type, for example `Condition`.
    pub resource_type: String,
    /// The resource's logical id.
    pub id: String,
    /// The resource's `meta.versionId`, when it carries one.
    pub version: Option<String>,
}

/// A CDM row, by its table and its surrogate id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WrittenRow {
    /// The CDM table.
    pub table: String,
    /// The row's surrogate id; for `PERSON` and `DEATH` the `person_id`.
    pub id: i32,
}

/// Returns the statements that create the `resource_origin` table and its
/// index by resource.
pub(super) fn origin_ddl(bridge: &SchemaName) -> [String; 2] {
    [
        format!(
            "CREATE TABLE IF NOT EXISTS {bridge}.resource_origin (
                versioned_object_uid text PRIMARY KEY,
                resource_type text NOT NULL,
                resource_id text NOT NULL,
                resource_version text)"
        ),
        format!(
            "CREATE INDEX IF NOT EXISTS resource_origin_resource
                ON {bridge}.resource_origin (resource_type, resource_id)"
        ),
    ]
}

/// Records that the composition `versioned_object_uid` was mapped from
/// `origin`, replacing what an earlier version recorded.
pub(super) async fn record(
    transaction: &Transaction<'_>,
    bridge: &SchemaName,
    versioned_object_uid: &str,
    origin: &ResourceOrigin,
) -> Result<(), WriteError> {
    transaction
        .execute(
            &format!(
                "INSERT INTO {bridge}.resource_origin
                    (versioned_object_uid, resource_type, resource_id, resource_version)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (versioned_object_uid) DO UPDATE SET
                    resource_type = EXCLUDED.resource_type,
                    resource_id = EXCLUDED.resource_id,
                    resource_version = EXCLUDED.resource_version"
            ),
            &[
                &versioned_object_uid,
                &origin.resource_type,
                &origin.id,
                &origin.version,
            ],
        )
        .await
        .map_err(database(Step::Record))?;
    Ok(())
}

/// Forgets the origin of the composition `versioned_object_uid`.
pub(super) async fn forget(
    transaction: &Transaction<'_>,
    bridge: &SchemaName,
    versioned_object_uid: &str,
) -> Result<(), WriteError> {
    transaction
        .execute(
            &format!("DELETE FROM {bridge}.resource_origin WHERE versioned_object_uid = $1"),
            &[&versioned_object_uid],
        )
        .await
        .map_err(database(Step::Record))?;
    Ok(())
}

impl CdmWriter {
    /// Returns the FHIR resources the row `id` of `table` was mapped from,
    /// ordered by type and id; a `PERSON` or `DEATH` row may have several
    /// writers, and a row written from a CDR composition has none.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Database`] with [`Step::Read`] when PostgreSQL
    /// refuses the lookup.
    pub async fn origins_of_row(
        &self,
        table: &str,
        id: i32,
    ) -> Result<Vec<ResourceOrigin>, WriteError> {
        let rows = self
            .client
            .query(
                &format!(
                    "SELECT DISTINCT o.resource_type, o.resource_id, o.resource_version
                     FROM {bridge}.record AS r
                     JOIN {bridge}.resource_origin AS o
                       ON o.versioned_object_uid = r.versioned_object_uid
                     WHERE r.cdm_table = $1 AND r.surrogate_id = $2
                     ORDER BY o.resource_type, o.resource_id",
                    bridge = self.bridge
                ),
                &[&table, &id],
            )
            .await
            .map_err(database(Step::Read))?;
        rows.iter()
            .map(|row| {
                Ok(ResourceOrigin {
                    resource_type: row.try_get(0).map_err(database(Step::Read))?,
                    id: row.try_get(1).map_err(database(Step::Read))?,
                    version: row.try_get(2).map_err(database(Step::Read))?,
                })
            })
            .collect()
    }

    /// Returns every CDM row the FHIR resource `resource_type`/`id` was
    /// mapped to, ordered by table and id.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Database`] with [`Step::Read`] when PostgreSQL
    /// refuses the lookup.
    pub async fn rows_of_resource(
        &self,
        resource_type: &str,
        id: &str,
    ) -> Result<Vec<WrittenRow>, WriteError> {
        let rows = self
            .client
            .query(
                &format!(
                    "SELECT DISTINCT r.cdm_table, r.surrogate_id
                     FROM {bridge}.resource_origin AS o
                     JOIN {bridge}.record AS r
                       ON r.versioned_object_uid = o.versioned_object_uid
                     WHERE o.resource_type = $1 AND o.resource_id = $2
                     ORDER BY r.cdm_table, r.surrogate_id",
                    bridge = self.bridge
                ),
                &[&resource_type, &id],
            )
            .await
            .map_err(database(Step::Read))?;
        rows.iter()
            .map(|row| {
                Ok(WrittenRow {
                    table: row.try_get(0).map_err(database(Step::Read))?,
                    id: row.try_get(1).map_err(database(Step::Read))?,
                })
            })
            .collect()
    }
}
