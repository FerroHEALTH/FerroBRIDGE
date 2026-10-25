// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The person of an EHR: its `person_id` in the person map and the `PERSON`
//! row every row of the person names.
//!
//! OMOP CDM v5.4 makes `person_id` a required foreign key to `PERSON` in every
//! clinical table, and "All Persons in a database needs one record in this
//! table, unless they fail data quality requirements specified in the ETL"
//! (`PERSON`, <https://ohdsi.github.io/CommonDataModel/cdm54.html#person>).
//! A `PERSON` row a mapping writes is the person's source. Without one, the
//! writer's [`PersonPolicy`] decides: a placeholder row, or a refusal.

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use tokio_postgres::Transaction;

use crate::ddl::SchemaName;
use crate::writer::CdmWriter;
use crate::writer::database;
use crate::writer::error::Step;
use crate::writer::error::WriteError;
use crate::writer::input::PersonPolicy;
use crate::writer::side_table::allocate;

/// The width of the `*_source_value` columns the writer fills itself
/// (`person_source_value`, `provider_source_value`,
/// `visit_detail_source_value`), `varchar(50)` in CDM v5.4.
pub(super) const SOURCE_VALUE_WIDTH: usize = 50;

/// Resolves `ehr_id` to its `person_id` in the person map, assigning one when
/// `create` allows.
pub(super) async fn mapped(
    transaction: &Transaction<'_>,
    bridge: &SchemaName,
    ehr_id: &HierObjectId,
    create: bool,
) -> Result<i32, WriteError> {
    let found = transaction
        .query_opt(
            &format!("SELECT person_id FROM {bridge}.person_map WHERE ehr_id = $1"),
            &[&ehr_id.value()],
        )
        .await
        .map_err(database(Step::Read))?;
    if let Some(row) = found {
        return row.try_get(0).map_err(database(Step::Read));
    }
    if !create {
        return Err(WriteError::UnknownPerson {
            ehr_id: ehr_id.clone(),
        });
    }
    let id = allocate(transaction, bridge, "person").await?;
    transaction
        .execute(
            &format!("INSERT INTO {bridge}.person_map (ehr_id, person_id) VALUES ($1, $2)"),
            &[&ehr_id.value(), &id],
        )
        .await
        .map_err(database(Step::Record))?;
    Ok(id)
}

/// Resolves `ehr_id` to the `person_id` of a `PERSON` row, writing the
/// placeholder row under [`PersonPolicy::CreateOnFirstSight`] when the person
/// has none.
///
/// Under [`PersonPolicy::Existing`] an EHR whose person has no `PERSON` row
/// is refused, whether or not the person map holds it.
pub(super) async fn person_id(
    transaction: &Transaction<'_>,
    cdm: &SchemaName,
    bridge: &SchemaName,
    ehr_id: &HierObjectId,
    policy: PersonPolicy,
) -> Result<i32, WriteError> {
    let create = policy == PersonPolicy::CreateOnFirstSight;
    let id = mapped(transaction, bridge, ehr_id, create).await?;
    let found = transaction
        .query_opt(
            &format!("SELECT 1 FROM {cdm}.person WHERE person_id = $1"),
            &[&id],
        )
        .await
        .map_err(database(Step::Read))?;
    if found.is_some() {
        return Ok(id);
    }
    if !create {
        return Err(WriteError::UnknownPerson {
            ehr_id: ehr_id.clone(),
        });
    }
    // NOTE: CDM v5.4 PERSON requires these four columns, 0 being "no matching concept";
    // no specification governs year_of_birth 0: our own design, the OMOCL corpus fallback.
    let source = ehr_id.value();
    let source = (source.chars().count() <= SOURCE_VALUE_WIDTH).then_some(source);
    transaction
        .execute(
            &format!(
                "INSERT INTO {cdm}.person (person_id, gender_concept_id, year_of_birth,
                    race_concept_id, ethnicity_concept_id, person_source_value)
                 VALUES ($1, 0, 0, 0, 0, $2)"
            ),
            &[&id, &source],
        )
        .await
        .map_err(database(Step::Insert("person")))?;
    Ok(id)
}

/// Records the columns of the rows of `table`, `person` or `death`, that
/// `persons` names, as the composition `versioned_object_uid` just wrote
/// them, as the newest writes of those rows in `<table>_row`.
///
/// [`restore_rows`] reads them back when a newer writer of the row goes.
pub(super) async fn record_rows(
    transaction: &Transaction<'_>,
    cdm: &SchemaName,
    bridge: &SchemaName,
    table: &'static str,
    versioned_object_uid: &str,
    persons: &[i32],
) -> Result<(), WriteError> {
    if persons.is_empty() {
        return Ok(());
    }
    // NOTE: no specification governs this: our own design; a sequence orders the writes,
    // so the newest writer that remains is the one recorded last.
    transaction
        .execute(
            &format!(
                "INSERT INTO {bridge}.{table}_row (versioned_object_uid, person_id, written, columns)
                 SELECT $1, p.person_id, nextval('{bridge}.person_row_written_seq'), to_jsonb(p)
                 FROM {cdm}.{table} AS p WHERE p.person_id = ANY($2) ORDER BY p.person_id"
            ),
            &[&versioned_object_uid, &persons],
        )
        .await
        .map_err(database(Step::Record))?;
    Ok(())
}

/// Rewrites the row of `table`, `person` or `death`, of each of `persons`
/// from the newest write of it that the side table still holds.
///
/// A person whose writers all go keeps its row for [`retire_persons`] or
/// [`retire_deaths`] to decide. A writer committed before the side table recorded columns has
/// none, and every writer that has columns was committed after it, so a
/// person left with only such writers keeps the row it has.
///
/// [`retire_persons`]: crate::writer::side_table::retire_persons
/// [`retire_deaths`]: crate::writer::side_table::retire_deaths
pub(super) async fn restore_rows(
    transaction: &Transaction<'_>,
    cdm: &SchemaName,
    bridge: &SchemaName,
    table: &'static str,
    persons: &[i32],
) -> Result<(), WriteError> {
    if persons.is_empty() {
        return Ok(());
    }
    // NOTE: no specification governs this: our own design; CDM v5.4 PERSON and DEATH hold
    // one row per person, so the newest writer that remains gives it its columns.
    let newest = format!(
        "SELECT DISTINCT ON (person_id) person_id, columns FROM {bridge}.{table}_row
         WHERE person_id = ANY($1) ORDER BY person_id, written DESC"
    );
    transaction
        .execute(
            &format!(
                "DELETE FROM {cdm}.{table} AS p
                 WHERE p.person_id IN (SELECT person_id FROM ({newest}) AS n)"
            ),
            &[&persons],
        )
        .await
        .map_err(database(Step::Delete(table)))?;
    transaction
        .execute(
            &format!(
                "INSERT INTO {cdm}.{table}
                 SELECT (jsonb_populate_record(NULL::{cdm}.{table}, n.columns)).*
                 FROM ({newest}) AS n"
            ),
            &[&persons],
        )
        .await
        .map_err(database(Step::Insert(table)))?;
    Ok(())
}

/// The `PERSON` rows of a CDM schema, and how many of them are placeholders.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Persons {
    /// `PERSON` rows.
    pub total: u64,
    /// `PERSON` rows no mapping wrote: the placeholders of persons with no
    /// `PERSON` source.
    pub placeholders: u64,
}

impl CdmWriter {
    /// Counts the `PERSON` rows, and those no live composition writes.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Database`] when the count is refused.
    pub async fn persons(&self) -> Result<Persons, WriteError> {
        let row = self
            .client
            .query_one(
                &format!(
                    "SELECT count(*), count(*) FILTER (WHERE NOT EXISTS (
                        SELECT 1 FROM {bridge}.record AS r
                        WHERE r.cdm_table = 'person' AND r.surrogate_id = p.person_id))
                     FROM {cdm}.person AS p",
                    bridge = self.bridge,
                    cdm = self.cdm
                ),
                &[],
            )
            .await
            .map_err(database(Step::Read))?;
        let total: i64 = row.try_get(0).map_err(database(Step::Read))?;
        let placeholders: i64 = row.try_get(1).map_err(database(Step::Read))?;
        Ok(Persons {
            total: total.unsigned_abs(),
            placeholders: placeholders.unsigned_abs(),
        })
    }
}
