// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `PERSON` row belongs to the person: updating or removing the
//! composition that wrote it keeps it while any row of the person remains,
//! because OMOP CDM v5.4 makes `person_id` a foreign key to `PERSON` in every
//! clinical table (<https://ohdsi.github.io/CommonDataModel/cdm54.html>).
//! Which writer's columns win is FerroBRIDGE's own design.

use ferrobridge_testkit::containers;
use omop_cdm::graph::{
    RecordGraph, key::ArchetypeRootPath, key::Discriminator, key::MappingName, key::OccurrencePath,
    key::RecordKey, key::Source, row::Row, row::Value,
};
use omop_cdm::writer::{input::PersonPolicy, input::RunId, removal::Removal};
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use std::error::Error;

use super::{COMPOSITION, EHR, count, database, measurement, reader};

/// The second synthetic versioned composition, of the same EHR.
const OTHER: &str = "1f7f3c1a-2b64-4a3c-9d0e-4a2f5c7b9e01";

/// Returns version `version` of the composition `uid`.
fn source_of(uid: &str, version: u32) -> Result<Source, Box<dyn Error>> {
    Ok(Source::new(
        HierObjectId::new(EHR)?,
        ObjectVersionId::new(format!("{uid}::ferrobridge.test::{version}"))?,
    ))
}

/// Returns the key of the node at `occurrence` of `source` under `mapping`.
fn key(source: &Source, mapping: &str, occurrence: &str) -> Result<RecordKey, Box<dyn Error>> {
    Ok(RecordKey::new(
        source,
        ArchetypeRootPath::new("/content[openEHR-EHR-ADMIN_ENTRY.synthetic.v1]")?,
        OccurrencePath::new(occurrence)?,
        Discriminator::new(MappingName::new(mapping)?, 0, 0),
    ))
}

/// Returns the graph of version `version` of `uid`: one measurement, and a
/// `PERSON` row born in `born` when given.
fn graph(uid: &str, version: u32, born: Option<i32>) -> Result<RecordGraph, Box<dyn Error>> {
    let source = source_of(uid, version)?;
    let mut graph = RecordGraph::new(source.clone());
    if let Some(year) = born {
        graph.push_row(
            Row::builder("person", key(&source, "Person_v1", "/")?)?
                .value("gender_concept_id", Value::Integer(8532))?
                .value("year_of_birth", Value::Integer(year))?
                .value("race_concept_id", Value::Integer(0))?
                .value("ethnicity_concept_id", Value::Integer(0))?
                .build()?,
        )?;
    }
    graph.push_row(measurement(
        key(&source, "Laboratory_test_result_v1", "/items[1]")?,
        1001,
        Some(4.5),
    )?)?;
    Ok(graph)
}

/// Returns the rows of every clinical table the test writes whose person is
/// missing.
async fn orphans(client: &tokio_postgres::Client) -> Result<i64, Box<dyn Error>> {
    count(
        client,
        "cdm.measurement m WHERE NOT EXISTS
            (SELECT 1 FROM cdm.person p WHERE p.person_id = m.person_id)",
    )
    .await
}

/// Returns the `year_of_birth` of every `PERSON` row.
async fn births(client: &tokio_postgres::Client) -> Result<Vec<i32>, Box<dyn Error>> {
    Ok(client
        .query(
            "SELECT year_of_birth FROM cdm.person ORDER BY person_id",
            &[],
        )
        .await?
        .iter()
        .map(|row| row.get(0))
        .collect())
}

#[tokio::test]
async fn an_update_without_the_person_row_keeps_it_and_a_later_one_rewrites_it()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    let run = RunId::new("run-1")?;
    database
        .writer
        .commit(&graph(COMPOSITION, 1, Some(1970))?, &run)
        .await?;
    database
        .writer
        .commit(&graph(OTHER, 1, None)?, &run)
        .await?;
    let client = reader(&database.postgres).await?;
    assert_eq!(vec![1970], births(&client).await?);

    database
        .writer
        .commit(&graph(COMPOSITION, 2, None)?, &run)
        .await?;
    assert_eq!(
        vec![1970],
        births(&client).await?,
        "the person keeps its row while its rows remain"
    );
    assert_eq!(0, orphans(&client).await?);

    database
        .writer
        .commit(&graph(COMPOSITION, 3, Some(1971))?, &run)
        .await?;
    assert_eq!(
        vec![1971],
        births(&client).await?,
        "the newest version that writes the person rewrites its columns"
    );
    assert_eq!(0, orphans(&client).await?);
    Ok(())
}

#[tokio::test]
async fn a_removal_keeps_the_person_while_another_composition_remains_and_then_removes_it()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    let run = RunId::new("run-1")?;
    database
        .writer
        .commit(&graph(COMPOSITION, 1, Some(1970))?, &run)
        .await?;
    database
        .writer
        .commit(&graph(OTHER, 1, None)?, &run)
        .await?;
    let client = reader(&database.postgres).await?;

    let watermarks = database.writer.watermarks_after(None, 10).await?;
    let writer_of_person = watermarks
        .iter()
        .find(|committed| committed.versioned_object_uid().value() == COMPOSITION)
        .ok_or("the composition that wrote the person")?;
    let other = watermarks
        .iter()
        .find(|committed| committed.versioned_object_uid().value() == OTHER)
        .ok_or("the other composition")?;

    let first = database.writer.remove(writer_of_person).await?;
    let Removal::Removed(rows) = first else {
        return Err(format!("expected a removal, got {first:?}").into());
    };
    assert_eq!(
        None,
        rows.get("person"),
        "the other composition's row remains"
    );
    assert_eq!(vec![1970], births(&client).await?);
    assert_eq!(0, orphans(&client).await?);

    let last = database.writer.remove(other).await?;
    let Removal::Removed(rows) = last else {
        return Err(format!("expected a removal, got {last:?}").into());
    };
    assert_eq!(Some(&1), rows.get("person"), "no row of the person remains");
    assert!(births(&client).await?.is_empty());
    assert_eq!(
        1,
        count(&client, "ferrobridge.person_map").await?,
        "the EHR keeps its person_id for a later row"
    );
    Ok(())
}
