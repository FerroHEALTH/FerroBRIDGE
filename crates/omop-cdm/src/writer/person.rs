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

/// The width of `person.person_source_value`, `varchar(50)` in CDM v5.4.
const SOURCE_VALUE_WIDTH: usize = 50;

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
