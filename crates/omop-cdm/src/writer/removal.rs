// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The compositions the side table holds, and the removal of one of them.
//!
//! A run that learns a composition was deleted at its source removes the rows
//! the side table attributes to it. The CDM has no deletion semantics of its
//! own (<https://ohdsi.github.io/CommonDataModel/cdm54.html>), so no
//! specification governs this: our own design. The removal is one
//! transaction, the same unit as [`CdmWriter::commit`].

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;

use crate::graph::key::VisitKey;
use crate::meta::CdmSchema;
use crate::writer::CdmWriter;
use crate::writer::database;
use crate::writer::error::Step;
use crate::writer::error::WriteError;
use crate::writer::person::restore_rows;
use crate::writer::quoted;
use crate::writer::side_table::delete_earlier;
use crate::writer::side_table::earlier_rows;
use crate::writer::side_table::retire_deaths;
use crate::writer::side_table::retire_persons;

/// One composition a watermark names: the versioned composition, its EHR and
/// the version last committed.
#[derive(Debug, Clone, PartialEq)]
pub struct Committed {
    versioned_object_uid: HierObjectId,
    ehr_id: HierObjectId,
    version_uid: ObjectVersionId,
}

impl Committed {
    /// Returns the versioned composition.
    #[must_use]
    pub fn versioned_object_uid(&self) -> &HierObjectId {
        &self.versioned_object_uid
    }

    /// Returns the EHR the composition belongs to.
    #[must_use]
    pub fn ehr_id(&self) -> &HierObjectId {
        &self.ehr_id
    }

    /// Returns the version last committed.
    #[must_use]
    pub fn version_uid(&self) -> &ObjectVersionId {
        &self.version_uid
    }
}

/// What a removal did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removal {
    /// The composition's rows, links and watermark are gone; the rows
    /// deleted, by CDM table, `fact_relationship` included.
    Removed(BTreeMap<&'static str, u64>),
    /// The watermark no longer names the version the caller read, so a
    /// commit overtook the removal and nothing was deleted.
    Superseded,
}

impl CdmWriter {
    /// Returns at most `limit` watermarks, ordered by versioned composition,
    /// starting after `after` when given.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Database`] when the side table cannot be read,
    /// [`WriteError::ObjectId`] when it holds an identifier that is no
    /// `HIER_OBJECT_ID`, and [`WriteError::Version`] when its version is no
    /// `OBJECT_VERSION_ID`.
    pub async fn watermarks_after(
        &self,
        after: Option<&HierObjectId>,
        limit: u32,
    ) -> Result<Vec<Committed>, WriteError> {
        let rows = self
            .client
            .query(
                &format!(
                    "SELECT versioned_object_uid, ehr_id, version_uid FROM {}.watermark
                     WHERE $1::text IS NULL OR versioned_object_uid > $1
                     ORDER BY versioned_object_uid LIMIT $2",
                    self.bridge
                ),
                &[&after.map(HierObjectId::value), &i64::from(limit)],
            )
            .await
            .map_err(database(Step::Read))?;
        let mut committed = Vec::with_capacity(rows.len());
        for row in rows {
            let versioned: String = row.try_get(0).map_err(database(Step::Read))?;
            let ehr: String = row.try_get(1).map_err(database(Step::Read))?;
            let version: String = row.try_get(2).map_err(database(Step::Read))?;
            committed.push(Committed {
                versioned_object_uid: object_id(versioned)?,
                ehr_id: object_id(ehr)?,
                version_uid: ObjectVersionId::new(version.as_str()).map_err(|source| {
                    WriteError::Version {
                        value: version.clone(),
                        source,
                    }
                })?,
            });
        }
        Ok(committed)
    }

    /// Removes the rows, the links and the watermark of the composition
    /// `committed` names, while its watermark still names that version.
    ///
    /// In one transaction, under the same per-composition lock a commit
    /// takes: the watermark is deleted when it names `committed`'s version,
    /// then every row and link the side table attributes to the composition.
    /// A `PERSON` row the composition wrote takes the columns of the newest
    /// writer that remains, and goes once no writer and no row of the person
    /// remain. A second removal of the same composition finds no watermark and
    /// answers [`Removal::Superseded`]; the person map is kept, so the EHR
    /// keeps its `person_id`.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Database`] for any statement PostgreSQL refuses
    /// and [`WriteError::SideTable`] for a side table this writer did not
    /// write; the transaction is rolled back and nothing is deleted.
    pub async fn remove(&mut self, committed: &Committed) -> Result<Removal, WriteError> {
        let Self {
            client,
            cdm,
            bridge,
            ..
        } = self;
        let transaction = client.transaction().await.map_err(database(Step::Begin))?;
        let versioned_object_uid = committed.versioned_object_uid.value();
        // NOTE: PostgreSQL docs, 9.28.10 "Advisory Lock Functions": the commit's own
        // transaction-level lock serializes a removal with a commit of the composition.
        transaction
            .execute(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&versioned_object_uid],
            )
            .await
            .map_err(database(Step::Lock))?;
        let removed = transaction
            .execute(
                &format!(
                    "DELETE FROM {bridge}.watermark
                     WHERE versioned_object_uid = $1 AND version_uid = $2"
                ),
                &[&versioned_object_uid, &committed.version_uid.value()],
            )
            .await
            .map_err(database(Step::Watermark))?;
        if removed == 0 {
            transaction
                .rollback()
                .await
                .map_err(database(Step::Commit))?;
            return Ok(Removal::Superseded);
        }
        let earlier = earlier_rows(&transaction, bridge, versioned_object_uid).await?;
        let deleted =
            delete_earlier(&transaction, cdm, bridge, versioned_object_uid, &earlier).await?;
        let written: Vec<i32> = deleted.written.iter().copied().collect();
        restore_rows(&transaction, cdm, bridge, "person", &written).await?;
        let deaths: Vec<i32> = deleted.deaths.iter().copied().collect();
        restore_rows(&transaction, cdm, bridge, "death", &deaths).await?;
        let mut rows = deleted.rows;
        let retired = retire_deaths(&transaction, cdm, bridge, &deleted.deaths).await?;
        if retired > 0 {
            rows.insert("death", retired);
        }
        let persons = retire_persons(&transaction, cdm, bridge, &deleted.persons).await?;
        if persons > 0 {
            rows.insert("person", persons);
        }
        transaction.commit().await.map_err(database(Step::Commit))?;
        Ok(Removal::Removed(rows))
    }
}

/// What a visit retirement did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VisitRetirement {
    /// Visits the visit query no longer answers, removed with their side
    /// table row.
    pub removed: u64,
    /// Visits the visit query no longer answers that a row still names,
    /// kept so no row names a missing visit.
    pub referenced: u64,
}

/// The column every row names its visit by.
const VISIT_OCCURRENCE_ID: &str = "visit_occurrence_id";

impl CdmWriter {
    /// Removes every visit the side table holds that `answered` does not
    /// name and no CDM row names any more, and returns what it did.
    ///
    /// `answered` is the whole answer of the visit query, so a visit missing
    /// from it is one the query no longer derives. A visit some row still
    /// names through `visit_occurrence_id` stays, counted as referenced. The
    /// person of a removed visit loses its `PERSON` row when no other row of
    /// the person remains. One transaction.
    ///
    /// # Errors
    ///
    /// Returns [`WriteError::Database`] for any statement PostgreSQL refuses;
    /// the transaction is rolled back and nothing is removed.
    pub async fn retire_visits(
        &mut self,
        answered: &[VisitKey],
    ) -> Result<VisitRetirement, WriteError> {
        let Self {
            client,
            cdm,
            bridge,
            ..
        } = self;
        let ehrs: Vec<&str> = answered.iter().map(|key| key.ehr_id().value()).collect();
        let sources: Vec<&str> = answered.iter().map(|key| key.source().as_str()).collect();
        let transaction = client.transaction().await.map_err(database(Step::Begin))?;
        let unanswered =
            "NOT EXISTS (SELECT 1 FROM UNNEST($1::text[], $2::text[]) AS a (ehr_id, visit_source)
                WHERE a.ehr_id = v.ehr_id AND a.visit_source = v.visit_source)";
        let mut referenced = Vec::new();
        for table in &crate::generated::TABLES {
            if table.cdm_schema == CdmSchema::Cdm
                && table.name != "visit_occurrence"
                && table.column(VISIT_OCCURRENCE_ID).is_some()
            {
                referenced.push(format!(
                    "EXISTS (SELECT 1 FROM {cdm}.{} AS t WHERE t.{} = v.visit_occurrence_id)",
                    quoted(table.name),
                    quoted(VISIT_OCCURRENCE_ID)
                ));
            }
        }
        let referenced = referenced.join(" OR ");
        let kept = transaction
            .query_one(
                &format!(
                    "SELECT count(*) FROM {bridge}.visit AS v WHERE {unanswered} AND ({referenced})"
                ),
                &[&ehrs, &sources],
            )
            .await
            .map_err(database(Step::Read))?;
        let kept: i64 = kept.try_get(0).map_err(database(Step::Read))?;
        let retired = transaction
            .query(
                &format!(
                    "DELETE FROM {bridge}.visit AS v WHERE {unanswered} AND NOT ({referenced})
                     RETURNING v.visit_occurrence_id"
                ),
                &[&ehrs, &sources],
            )
            .await
            .map_err(database(Step::Record))?;
        let mut ids = Vec::with_capacity(retired.len());
        for row in &retired {
            let id: i32 = row.try_get(0).map_err(database(Step::Record))?;
            ids.push(id);
        }
        let persons = transaction
            .query(
                &format!(
                    "DELETE FROM {cdm}.visit_occurrence WHERE visit_occurrence_id = ANY($1)
                     RETURNING person_id"
                ),
                &[&ids],
            )
            .await
            .map_err(database(Step::Delete("visit_occurrence")))?;
        let mut touched = BTreeSet::new();
        for row in &persons {
            let person: i32 = row
                .try_get(0)
                .map_err(database(Step::Delete("visit_occurrence")))?;
            touched.insert(person);
        }
        retire_persons(&transaction, cdm, bridge, &touched).await?;
        transaction.commit().await.map_err(database(Step::Commit))?;
        Ok(VisitRetirement {
            removed: u64::try_from(ids.len()).unwrap_or(u64::MAX),
            referenced: kept.unsigned_abs(),
        })
    }
}

/// Returns the `HIER_OBJECT_ID` the side table holds as `value`.
fn object_id(value: String) -> Result<HierObjectId, WriteError> {
    HierObjectId::new(value.as_str()).map_err(|source| WriteError::ObjectId { value, source })
}
