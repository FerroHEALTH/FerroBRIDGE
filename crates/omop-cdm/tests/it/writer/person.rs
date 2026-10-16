// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `PERSON` row belongs to the person: updating or removing the
//! composition that wrote it keeps it while any row of the person remains,
//! because OMOP CDM v5.4 makes `person_id` a foreign key to `PERSON` in every
//! clinical table (<https://ohdsi.github.io/CommonDataModel/cdm54.html>).
//! Which writer's columns win when several compositions write the row, and
//! the placeholder row of a person with no `PERSON` source, are
//! FerroBRIDGE's own design.

use ferrobridge_testkit::containers;
use omop_cdm::graph::{
    RecordGraph, key::ArchetypeRootPath, key::Discriminator, key::MappingName, key::OccurrencePath,
    key::RecordKey, key::Source, row::Row, row::Value,
};
use omop_cdm::meta::CdmSchema;
use omop_cdm::writer::{
    error::WriteError, input::PersonPolicy, input::RunId, person::Persons, removal::Removal,
};
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

/// Returns the rows of every CDM table that names a person whose `PERSON`
/// row is missing, over every table with a `person_id`.
async fn orphans_in_every_table(client: &tokio_postgres::Client) -> Result<i64, Box<dyn Error>> {
    let mut total = 0;
    for table in &omop_cdm::generated::TABLES {
        if table.cdm_schema == CdmSchema::Cdm
            && table.name != "person"
            && table.column("person_id").is_some()
        {
            total += count(
                client,
                &format!(
                    "cdm.{} t WHERE NOT EXISTS
                        (SELECT 1 FROM cdm.person p WHERE p.person_id = t.person_id)",
                    table.name
                ),
            )
            .await?;
        }
    }
    Ok(total)
}

/// Returns every `PERSON` row as its four required columns and its source
/// value.
async fn person_rows(
    client: &tokio_postgres::Client,
) -> Result<Vec<(i32, i32, i32, i32, Option<String>)>, Box<dyn Error>> {
    Ok(client
        .query(
            "SELECT gender_concept_id, year_of_birth, race_concept_id, ethnicity_concept_id,
                person_source_value
             FROM cdm.person ORDER BY person_id",
            &[],
        )
        .await?
        .iter()
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4)))
        .collect())
}

#[tokio::test]
async fn a_person_with_no_person_source_gets_the_placeholder_row() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    database
        .writer
        .commit(&graph(OTHER, 1, None)?, &RunId::new("run-1")?)
        .await?;
    let client = reader(&database.postgres).await?;
    assert_eq!(
        vec![(0, 0, 0, 0, Some(EHR.to_owned()))],
        person_rows(&client).await?,
        "every concept 0, year_of_birth 0, and the EHR as the source value"
    );
    assert_eq!(0, orphans_in_every_table(&client).await?);
    assert_eq!(
        Persons {
            total: 1,
            placeholders: 1,
        },
        database.writer.persons().await?
    );
    Ok(())
}

#[tokio::test]
async fn a_composition_writing_the_person_replaces_the_placeholder() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    let run = RunId::new("run-1")?;
    database
        .writer
        .commit(&graph(OTHER, 1, None)?, &run)
        .await?;
    let client = reader(&database.postgres).await?;
    let placeholder: i32 = client
        .query_one("SELECT person_id FROM cdm.person", &[])
        .await?
        .get(0);

    database
        .writer
        .commit(&graph(COMPOSITION, 1, Some(1970))?, &run)
        .await?;
    assert_eq!(vec![1970], births(&client).await?);
    let replaced: i32 = client
        .query_one("SELECT person_id FROM cdm.person", &[])
        .await?
        .get(0);
    assert_eq!(placeholder, replaced, "the person keeps its person_id");
    assert_eq!(0, orphans_in_every_table(&client).await?);
    assert_eq!(
        Persons {
            total: 1,
            placeholders: 0,
        },
        database.writer.persons().await?
    );
    Ok(())
}

#[tokio::test]
async fn the_existing_policy_takes_a_person_its_composition_writes() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::Existing).await?;
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
    assert_eq!(0, orphans_in_every_table(&client).await?);
    Ok(())
}

#[tokio::test]
async fn the_existing_policy_refuses_a_person_whose_row_is_gone() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::Existing).await?;
    let run = RunId::new("run-1")?;
    database
        .writer
        .commit(&graph(COMPOSITION, 1, Some(1970))?, &run)
        .await?;
    let committed = database
        .writer
        .watermarks_after(None, 10)
        .await?
        .into_iter()
        .next()
        .ok_or("the composition that wrote the person")?;
    database.writer.remove(&committed).await?;
    let client = reader(&database.postgres).await?;
    assert!(births(&client).await?.is_empty());
    assert_eq!(1, count(&client, "ferrobridge.person_map").await?);

    let error = database
        .writer
        .commit(&graph(OTHER, 1, None)?, &run)
        .await
        .expect_err("the person map names the EHR and no PERSON row does");
    assert!(
        matches!(error, WriteError::UnknownPerson { .. }),
        "{error:?}"
    );
    assert_eq!(0, count(&client, "cdm.measurement").await?);
    Ok(())
}

/// Returns the watermark of the composition `uid`.
async fn committed_of(
    writer: &omop_cdm::writer::CdmWriter,
    uid: &str,
) -> Result<omop_cdm::writer::removal::Committed, Box<dyn Error>> {
    Ok(writer
        .watermarks_after(None, 10)
        .await?
        .into_iter()
        .find(|committed| committed.versioned_object_uid().value() == uid)
        .ok_or("the composition's watermark")?)
}

/// Returns the number of compositions the side table records as writers of
/// a `PERSON` row.
async fn person_writers(client: &tokio_postgres::Client) -> Result<i64, Box<dyn Error>> {
    count(client, "ferrobridge.record WHERE cdm_table = 'person'").await
}

#[tokio::test]
async fn two_compositions_of_one_ehr_both_write_the_person_and_the_newest_wins()
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
        .commit(&graph(OTHER, 1, Some(1980))?, &run)
        .await?;
    let client = reader(&database.postgres).await?;
    assert_eq!(
        2,
        person_writers(&client).await?,
        "both writers are recorded"
    );
    assert_eq!(
        vec![1980],
        births(&client).await?,
        "the composition committed last rewrites the columns"
    );

    database
        .writer
        .commit(&graph(COMPOSITION, 2, Some(1971))?, &run)
        .await?;
    assert_eq!(
        vec![1971],
        births(&client).await?,
        "a newer version of either writer rewrites them"
    );
    assert_eq!(2, person_writers(&client).await?);
    assert_eq!(
        Persons {
            total: 1,
            placeholders: 0,
        },
        database.writer.persons().await?
    );
    assert_eq!(0, orphans_in_every_table(&client).await?);
    Ok(())
}

#[tokio::test]
async fn removing_either_writer_keeps_the_person_and_removing_both_removes_it()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    for first in [COMPOSITION, OTHER] {
        let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
        let run = RunId::new("run-1")?;
        database
            .writer
            .commit(&graph(COMPOSITION, 1, Some(1970))?, &run)
            .await?;
        database
            .writer
            .commit(&graph(OTHER, 1, Some(1980))?, &run)
            .await?;
        let client = reader(&database.postgres).await?;
        let second = if first == COMPOSITION {
            OTHER
        } else {
            COMPOSITION
        };

        let removal = database
            .writer
            .remove(&committed_of(&database.writer, first).await?)
            .await?;
        let Removal::Removed(rows) = removal else {
            return Err(format!("expected a removal, got {removal:?}").into());
        };
        assert_eq!(None, rows.get("person"), "{second} still writes the person");
        assert_eq!(1, births(&client).await?.len(), "removing {first} keeps it");
        assert_eq!(1, person_writers(&client).await?);
        assert_eq!(0, orphans_in_every_table(&client).await?);

        let removal = database
            .writer
            .remove(&committed_of(&database.writer, second).await?)
            .await?;
        let Removal::Removed(rows) = removal else {
            return Err(format!("expected a removal, got {removal:?}").into());
        };
        assert_eq!(Some(&1), rows.get("person"), "no writer and no row remain");
        assert!(births(&client).await?.is_empty());
        assert_eq!(0, person_writers(&client).await?);
    }
    Ok(())
}

#[tokio::test]
async fn a_bridge_schema_of_an_earlier_release_takes_several_writers_after_init()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    let client = reader(&database.postgres).await?;
    client
        .batch_execute(
            "DROP TABLE ferrobridge.record;
             CREATE TABLE ferrobridge.record (
                versioned_object_uid text NOT NULL,
                archetype_root_path text NOT NULL,
                occurrence_path text NOT NULL,
                cdm_table text NOT NULL,
                ehr_id text NOT NULL,
                mapping text NOT NULL,
                entry integer NOT NULL,
                branch integer NOT NULL,
                surrogate_id integer NOT NULL,
                PRIMARY KEY (versioned_object_uid, archetype_root_path, occurrence_path,
                    mapping, entry, branch, cdm_table),
                UNIQUE (cdm_table, surrogate_id))",
        )
        .await?;
    database.writer.init().await?;
    database.writer.init().await?;
    let run = RunId::new("run-1")?;
    database
        .writer
        .commit(&graph(COMPOSITION, 1, Some(1970))?, &run)
        .await?;
    database
        .writer
        .commit(&graph(OTHER, 1, Some(1980))?, &run)
        .await?;
    assert_eq!(2, person_writers(&client).await?);
    assert_eq!(vec![1980], births(&client).await?);

    let duplicate = client
        .execute(
            "INSERT INTO ferrobridge.record SELECT versioned_object_uid, '/other',
                occurrence_path, cdm_table, ehr_id, mapping, entry, branch, surrogate_id
             FROM ferrobridge.record WHERE cdm_table = 'measurement' LIMIT 1",
            &[],
        )
        .await;
    assert!(
        duplicate.is_err(),
        "an id stays unique in every table other than person"
    );
    Ok(())
}

/// The third synthetic versioned composition, of the same EHR.
const THIRD: &str = "6b2d9e4f-8a13-4c57-b0e2-9d4c1f3a7e52";

/// Returns the `PERSON` row a graph of [`graph`] born in `year` writes, as
/// [`person_rows`] reads it.
fn written_by(year: i32) -> (i32, i32, i32, i32, Option<String>) {
    (8532, year, 0, 0, None)
}

#[tokio::test]
async fn removing_the_newest_writer_hands_the_person_row_to_the_newest_writer_that_remains()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    let run = RunId::new("run-1")?;
    for (uid, year) in [(COMPOSITION, 1970), (OTHER, 1980), (THIRD, 1990)] {
        database
            .writer
            .commit(&graph(uid, 1, Some(year))?, &run)
            .await?;
    }
    let client = reader(&database.postgres).await?;
    assert_eq!(vec![written_by(1990)], person_rows(&client).await?);

    database
        .writer
        .remove(&committed_of(&database.writer, THIRD).await?)
        .await?;
    assert_eq!(
        vec![written_by(1980)],
        person_rows(&client).await?,
        "the newest writer that remains gives the row its columns"
    );
    assert_eq!(0, orphans_in_every_table(&client).await?);

    database
        .writer
        .remove(&committed_of(&database.writer, COMPOSITION).await?)
        .await?;
    assert_eq!(
        vec![written_by(1980)],
        person_rows(&client).await?,
        "removing an older writer keeps the newest writer's columns"
    );

    let removal = database
        .writer
        .remove(&committed_of(&database.writer, OTHER).await?)
        .await?;
    let Removal::Removed(rows) = removal else {
        return Err(format!("expected a removal, got {removal:?}").into());
    };
    assert_eq!(Some(&1), rows.get("person"), "no writer and no row remain");
    assert!(person_rows(&client).await?.is_empty());
    assert_eq!(0, person_writers(&client).await?);
    Ok(())
}

#[tokio::test]
async fn a_newest_writer_that_stops_writing_the_person_hands_the_row_to_the_writer_that_remains()
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
        .commit(&graph(OTHER, 1, Some(1980))?, &run)
        .await?;
    let client = reader(&database.postgres).await?;
    assert_eq!(vec![written_by(1980)], person_rows(&client).await?);

    database
        .writer
        .commit(&graph(OTHER, 2, None)?, &run)
        .await?;
    assert_eq!(
        vec![written_by(1970)],
        person_rows(&client).await?,
        "the writer that remains gives the row its columns"
    );
    assert_eq!(1, person_writers(&client).await?);
    assert_eq!(0, orphans_in_every_table(&client).await?);

    database
        .writer
        .commit(&graph(OTHER, 3, Some(1985))?, &run)
        .await?;
    assert_eq!(
        vec![written_by(1985)],
        person_rows(&client).await?,
        "a writer that writes the person again is the newest"
    );
    Ok(())
}
