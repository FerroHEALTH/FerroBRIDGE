// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The rows several compositions name: a provider and a visit detail.

use ferrobridge_testkit::containers;
use omop_cdm::graph::{
    RecordGraph, key::ProviderSource, key::RecordKey, key::Visit, key::VisitDetailKey,
    key::VisitDetailSource, key::VisitKey, key::VisitSource, row::Reference, row::Row, row::Value,
};
use omop_cdm::value::CdmDate;
use omop_cdm::writer::{
    error::WriteError, input::PersonPolicy, input::RunId, input::VisitConcepts,
};
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use std::error::Error;

use super::{EHR, analyte, count, database, reader, source};

/// Returns a drug exposure under `key` prescribed by the provider `source`,
/// in the detail `detail` of `visit` when one is given.
pub(super) fn drug_exposure(
    key: RecordKey,
    provider: &str,
    detail: Option<(&VisitKey, &str)>,
) -> Result<Row, Box<dyn Error>> {
    let mut builder = Row::builder("drug_exposure", key)?
        .reference("person_id", Reference::Person(HierObjectId::new(EHR)?))?
        .value("drug_concept_id", Value::Integer(1010))?
        .value(
            "drug_exposure_start_date",
            Value::Date(CdmDate::new("2026-06-15")?),
        )?
        .value(
            "drug_exposure_end_date",
            Value::Date(CdmDate::new("2026-06-16")?),
        )?
        .value("drug_type_concept_id", Value::Integer(32817))?
        .reference(
            "provider_id",
            Reference::Provider(ProviderSource::new(provider)?),
        )?;
    if let Some((visit, source)) = detail {
        builder = builder.reference(
            "visit_detail_id",
            Reference::VisitDetail(VisitDetailKey::new(
                visit.clone(),
                VisitDetailSource::new(source)?,
            )),
        )?;
    }
    Ok(builder.build()?)
}

#[tokio::test]
async fn a_drug_exposure_names_its_provider_and_every_commit_shares_the_row()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    let mut graph = RecordGraph::new(source(1)?);
    graph.push_row(drug_exposure(analyte(1)?, "synthetic-prescriber-1", None)?)?;
    graph.push_row(drug_exposure(analyte(2)?, "synthetic-prescriber-1", None)?)?;
    database
        .writer
        .commit(&graph, &RunId::new("run-1")?)
        .await?;
    database
        .writer
        .commit(&graph, &RunId::new("run-2")?)
        .await?;
    let client = reader(&database.postgres).await?;
    let providers = client
        .query(
            "SELECT provider_id, provider_source_value FROM cdm.provider",
            &[],
        )
        .await?;
    let [provider] = providers.as_slice() else {
        panic!("one PROVIDER row, got {}", providers.len());
    };
    assert_eq!(
        "synthetic-prescriber-1",
        provider.get::<_, &str>(1),
        "the source is kept"
    );
    let id: i32 = provider.get(0);
    let named: Vec<Option<i32>> = client
        .query(
            "SELECT provider_id FROM cdm.drug_exposure ORDER BY drug_exposure_id",
            &[],
        )
        .await?
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert_eq!(
        vec![Some(id), Some(id)],
        named,
        "both rows name the one provider"
    );
    Ok(())
}

#[tokio::test]
async fn a_visit_detail_is_created_from_its_visit_once() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    let visit = VisitKey::new(HierObjectId::new(EHR)?, VisitSource::new("encounter-1")?);
    database
        .writer
        .write_visits(
            &[Visit::new(
                visit.clone(),
                (CdmDate::new("2026-06-14")?, None),
                (CdmDate::new("2026-06-16")?, None),
            )?],
            VisitConcepts {
                visit_concept_id: 1001,
                visit_type_concept_id: 32817,
            },
        )
        .await?;
    let mut graph = RecordGraph::new(source(1)?);
    graph.push_row(drug_exposure(
        analyte(1)?,
        "synthetic-prescriber-1",
        Some((&visit, "ward-3")),
    )?)?;
    graph.push_row(drug_exposure(
        analyte(2)?,
        "synthetic-prescriber-1",
        Some((&visit, "ward-3")),
    )?)?;
    database
        .writer
        .commit(&graph, &RunId::new("run-1")?)
        .await?;
    let client = reader(&database.postgres).await?;
    let details = client
        .query(
            "SELECT visit_detail_id, visit_occurrence_id, visit_detail_concept_id,
                    visit_detail_start_date::text, visit_detail_end_date::text,
                    visit_detail_type_concept_id, visit_detail_source_value
             FROM cdm.visit_detail",
            &[],
        )
        .await?;
    let [detail] = details.as_slice() else {
        panic!("one VISIT_DETAIL row, got {}", details.len());
    };
    assert_eq!(1_i32, detail.get::<_, i32>(1), "it lies in the visit");
    assert_eq!(1001_i32, detail.get::<_, i32>(2));
    assert_eq!("2026-06-14", detail.get::<_, &str>(3));
    assert_eq!("2026-06-16", detail.get::<_, &str>(4));
    assert_eq!(32817_i32, detail.get::<_, i32>(5));
    assert_eq!("ward-3", detail.get::<_, &str>(6));
    let id: i32 = detail.get(0);
    let named: Vec<Option<i32>> = client
        .query("SELECT visit_detail_id FROM cdm.drug_exposure", &[])
        .await?
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert_eq!(vec![Some(id), Some(id)], named);
    Ok(())
}

#[tokio::test]
async fn a_visit_detail_of_an_unknown_visit_writes_nothing() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    let visit = VisitKey::new(HierObjectId::new(EHR)?, VisitSource::new("no-such-visit")?);
    let mut graph = RecordGraph::new(source(1)?);
    graph.push_row(drug_exposure(
        analyte(1)?,
        "synthetic-prescriber-1",
        Some((&visit, "ward-3")),
    )?)?;
    let refused = database
        .writer
        .commit(&graph, &RunId::new("run-1")?)
        .await
        .expect_err("the visit is unknown");
    assert!(
        matches!(refused, WriteError::UnknownVisit { .. }),
        "{refused:?}"
    );
    let client = reader(&database.postgres).await?;
    assert_eq!(0, count(&client, "cdm.drug_exposure").await?);
    assert_eq!(
        0,
        count(&client, "cdm.provider").await?,
        "the transaction rolled back"
    );
    Ok(())
}
