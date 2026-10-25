// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The rows several compositions name and none owns: a `PROVIDER` by its
//! source, and a `VISIT_DETAIL` of a derived visit by its source.
//!
//! Each is resolved inside the composition's transaction: the side table
//! names the id an earlier commit gave the key, and a key seen for the first
//! time gets a row with an id from its table's sequence. A `PROVIDER` row
//! carries its source in `provider_source_value`; a `VISIT_DETAIL` row takes
//! its person, concept, dates and type concept from the `VISIT_OCCURRENCE`
//! it lies in, the visit's row at the time the detail is created. The CDM
//! requires `visit_detail_concept_id`, both dates and the type concept, and
//! the visit is the one record of them the bridge holds. No specification
//! governs either row's derivation: our own design over the CDM's `PROVIDER`
//! and `VISIT_DETAIL` definitions
//! (<https://ohdsi.github.io/CommonDataModel/cdm54.html>).

use tokio_postgres::Transaction;

use crate::ddl::SchemaName;
use crate::graph::key::ProviderSource;
use crate::graph::key::VisitDetailKey;
use crate::writer::database;
use crate::writer::error::Step;
use crate::writer::error::WriteError;
use crate::writer::person::SOURCE_VALUE_WIDTH;
use crate::writer::pseudonym::Pseudonymiser;
use crate::writer::side_table::allocate;

/// Returns `source` as a `varchar(50)` source value column takes it: its
/// pseudonym under `pseudonymiser`, and NULL when it does not fit.
fn fitting(source: &str, pseudonymiser: Option<&Pseudonymiser>) -> Option<String> {
    let source = pseudonymiser.map_or_else(|| source.to_owned(), |p| p.pseudonym(source));
    (source.chars().count() <= SOURCE_VALUE_WIDTH).then_some(source)
}

/// Returns the `provider_id` of the provider `source` names, creating its
/// `PROVIDER` row the first time the source is seen.
pub(super) async fn provider_id(
    transaction: &Transaction<'_>,
    cdm: &SchemaName,
    bridge: &SchemaName,
    source: &ProviderSource,
    pseudonymiser: Option<&Pseudonymiser>,
) -> Result<i32, WriteError> {
    let found = transaction
        .query_opt(
            &format!("SELECT provider_id FROM {bridge}.provider WHERE source = $1"),
            &[&source.as_str()],
        )
        .await
        .map_err(database(Step::Read))?;
    if let Some(row) = found {
        return row.try_get(0).map_err(database(Step::Read));
    }
    let id = allocate(transaction, bridge, "provider").await?;
    transaction
        .execute(
            &format!(
                "INSERT INTO {cdm}.provider (provider_id, provider_source_value) VALUES ($1, $2)"
            ),
            &[&id, &fitting(source.as_str(), pseudonymiser)],
        )
        .await
        .map_err(database(Step::Insert("provider")))?;
    transaction
        .execute(
            &format!("INSERT INTO {bridge}.provider (source, provider_id) VALUES ($1, $2)"),
            &[&source.as_str(), &id],
        )
        .await
        .map_err(database(Step::Record))?;
    Ok(id)
}

/// Returns the `visit_detail_id` of the detail `key` names, creating its
/// `VISIT_DETAIL` row from its visit the first time the key is seen.
///
/// # Errors
///
/// Returns [`WriteError::UnknownVisit`] when the visit has no
/// `VISIT_OCCURRENCE` row, and a database error otherwise.
pub(super) async fn visit_detail_id(
    transaction: &Transaction<'_>,
    cdm: &SchemaName,
    bridge: &SchemaName,
    key: &VisitDetailKey,
    pseudonymiser: Option<&Pseudonymiser>,
) -> Result<i32, WriteError> {
    let visit = key.visit();
    let ehr_id = visit.ehr_id().value();
    let visit_source = visit.source().as_str();
    let source = key.source().as_str();
    let found = transaction
        .query_opt(
            &format!(
                "SELECT visit_detail_id FROM {bridge}.visit_detail
                 WHERE ehr_id = $1 AND visit_source = $2 AND source = $3"
            ),
            &[&ehr_id, &visit_source, &source],
        )
        .await
        .map_err(database(Step::Read))?;
    if let Some(row) = found {
        return row.try_get(0).map_err(database(Step::Read));
    }
    let id = allocate(transaction, bridge, "visit_detail").await?;
    let created = transaction
        .execute(
            &format!(
                "INSERT INTO {cdm}.visit_detail (visit_detail_id, person_id,
                     visit_detail_concept_id, visit_detail_start_date,
                     visit_detail_start_datetime, visit_detail_end_date,
                     visit_detail_end_datetime, visit_detail_type_concept_id,
                     visit_detail_source_value, visit_occurrence_id)
                 SELECT $1, o.person_id, o.visit_concept_id, o.visit_start_date,
                     o.visit_start_datetime, o.visit_end_date, o.visit_end_datetime,
                     o.visit_type_concept_id, $2, o.visit_occurrence_id
                 FROM {bridge}.visit AS v
                 JOIN {cdm}.visit_occurrence AS o ON o.visit_occurrence_id = v.visit_occurrence_id
                 WHERE v.ehr_id = $3 AND v.visit_source = $4"
            ),
            &[&id, &fitting(source, pseudonymiser), &ehr_id, &visit_source],
        )
        .await
        .map_err(database(Step::Insert("visit_detail")))?;
    if created == 0 {
        return Err(WriteError::UnknownVisit { key: visit.clone() });
    }
    transaction
        .execute(
            &format!(
                "INSERT INTO {bridge}.visit_detail (ehr_id, visit_source, source, visit_detail_id)
                 VALUES ($1, $2, $3, $4)"
            ),
            &[&ehr_id, &visit_source, &source, &id],
        )
        .await
        .map_err(database(Step::Record))?;
    Ok(id)
}
