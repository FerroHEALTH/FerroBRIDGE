// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The retirement of the visits the visit query no longer answers: a visit
//! no row names goes with its side table row, a visit a row still names
//! stays, so no row names a missing visit. No specification governs visit
//! derivation: FerroBRIDGE's own design.

use ferrobridge_testkit::containers;
use omop_cdm::graph::{
    key::Visit, key::VisitKey, key::VisitSource, row::Reference, row::Row, row::Value,
};
use omop_cdm::value::CdmDate;
use omop_cdm::writer::{
    input::PersonPolicy, input::RunId, input::VisitConcepts, removal::VisitRetirement,
};
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use std::error::Error;

use super::{EHR, analyte, count, database, laboratory, reader};

/// Returns the visit `source` of the synthetic EHR, on 14 to 16 June.
fn visit(source: &str) -> Result<Visit, Box<dyn Error>> {
    Ok(Visit::new(
        VisitKey::new(HierObjectId::new(EHR)?, VisitSource::new(source)?),
        (CdmDate::new("2026-06-14")?, None),
        (CdmDate::new("2026-06-16")?, None),
    )?)
}

/// The concepts every visit carries.
const CONCEPTS: VisitConcepts = VisitConcepts {
    visit_concept_id: 1001,
    visit_type_concept_id: 32817,
};

#[tokio::test]
async fn an_unanswered_visit_no_row_names_is_removed_and_a_named_one_kept()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    let (gone, named, answered) = (visit("ward-a")?, visit("ward-b")?, visit("ward-c")?);
    database
        .writer
        .write_visits(&[gone, named.clone(), answered.clone()], CONCEPTS)
        .await?;
    let mut graph = laboratory(1, 0)?;
    graph.push_row(
        Row::builder("measurement", analyte(1)?)?
            .reference("person_id", Reference::Person(HierObjectId::new(EHR)?))?
            .value("measurement_concept_id", Value::Integer(1001))?
            .value("measurement_date", Value::Date(CdmDate::new("2026-06-15")?))?
            .value("measurement_type_concept_id", Value::Integer(32817))?
            .reference("visit_occurrence_id", Reference::Visit(named.key().clone()))?
            .build()?,
    )?;
    database
        .writer
        .commit(&graph, &RunId::new("run-1")?)
        .await?;

    let retired = database
        .writer
        .retire_visits(std::slice::from_ref(answered.key()))
        .await?;
    assert_eq!(
        VisitRetirement {
            removed: 1,
            referenced: 1,
        },
        retired
    );
    let client = reader(&database.postgres).await?;
    let mut sources: Vec<String> = client
        .query("SELECT visit_source_value FROM cdm.visit_occurrence", &[])
        .await?
        .iter()
        .map(|row| row.get(0))
        .collect();
    sources.sort();
    assert_eq!(vec!["ward-b", "ward-c"], sources);
    assert_eq!(2, count(&client, "ferrobridge.visit").await?);
    assert_eq!(
        0,
        count(
            &client,
            "cdm.measurement m WHERE m.visit_occurrence_id IS NOT NULL AND NOT EXISTS
                (SELECT 1 FROM cdm.visit_occurrence v
                 WHERE v.visit_occurrence_id = m.visit_occurrence_id)"
        )
        .await?,
        "no row names a missing visit"
    );
    assert_eq!(
        VisitRetirement {
            removed: 0,
            referenced: 1,
        },
        database
            .writer
            .retire_visits(std::slice::from_ref(answered.key()))
            .await?,
        "a second retirement removes nothing more and still keeps the named visit"
    );
    Ok(())
}
