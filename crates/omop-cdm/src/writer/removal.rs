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

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;

use crate::writer::CdmWriter;
use crate::writer::database;
use crate::writer::error::Step;
use crate::writer::error::WriteError;
use crate::writer::side_table::delete_earlier;
use crate::writer::side_table::earlier_rows;

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
    /// A second removal of the same composition finds no watermark and
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
        let rows =
            delete_earlier(&transaction, cdm, bridge, versioned_object_uid, &earlier).await?;
        transaction.commit().await.map_err(database(Step::Commit))?;
        Ok(Removal::Removed(rows))
    }
}

/// Returns the `HIER_OBJECT_ID` the side table holds as `value`.
fn object_id(value: String) -> Result<HierObjectId, WriteError> {
    HierObjectId::new(value.as_str()).map_err(|source| WriteError::ObjectId { value, source })
}
