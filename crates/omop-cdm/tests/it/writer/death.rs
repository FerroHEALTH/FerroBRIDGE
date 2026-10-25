// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `DEATH` row several compositions of a person may write.

use ferrobridge_testkit::containers;
use omop_cdm::graph::{
    RecordGraph, key::ArchetypeRootPath, key::Discriminator, key::MappingName, key::OccurrencePath,
    key::RecordKey, key::Source, row::Reference, row::Row, row::Value,
};
use omop_cdm::value::CdmDate;
use omop_cdm::writer::{CdmWriter, input::PersonPolicy, input::RunId, removal::Committed};
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use std::error::Error;

use super::{COMPOSITION, EHR, database, reader};

/// The second synthetic composition of the EHR, beside [`COMPOSITION`].
const OTHER_COMPOSITION: &str = "1b3c8a2e-5f4d-4e6a-9b7c-2d1e0f9a8b7c";

/// Returns the graph of version `version` of `composition` with one `DEATH`
/// row on `date`, or none when `date` is `None`.
fn death(
    composition: &str,
    version: u32,
    date: Option<&str>,
) -> Result<RecordGraph, Box<dyn Error>> {
    let source = Source::new(
        HierObjectId::new(EHR)?,
        ObjectVersionId::new(format!("{composition}::ferrobridge.test::{version}"))?,
    );
    let mut graph = RecordGraph::new(source.clone());
    if let Some(date) = date {
        graph.push_row(
            Row::builder(
                "death",
                RecordKey::new(
                    &source,
                    ArchetypeRootPath::new("/content[openEHR-EHR-EVALUATION.death_summary.v1]")?,
                    OccurrencePath::new("/")?,
                    Discriminator::new(MappingName::new("Death_summary_v1")?, 0, 0),
                ),
            )?
            .reference("person_id", Reference::Person(HierObjectId::new(EHR)?))?
            .value("death_date", Value::Date(CdmDate::new(date)?))?
            .build()?,
        )?;
    }
    Ok(graph)
}

/// Returns every `DEATH` row as its date.
async fn deaths(client: &tokio_postgres::Client) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(client
        .query(
            "SELECT death_date::text FROM cdm.death ORDER BY death_date",
            &[],
        )
        .await?
        .iter()
        .map(|row| row.get(0))
        .collect())
}

/// Returns the watermark of `composition`.
async fn watermark_of(writer: &CdmWriter, composition: &str) -> Result<Committed, Box<dyn Error>> {
    writer
        .watermarks_after(None, 10)
        .await?
        .into_iter()
        .find(|committed| committed.versioned_object_uid().value() == composition)
        .ok_or_else(|| format!("no watermark of {composition}").into())
}

#[tokio::test]
async fn the_newest_writer_of_a_death_row_wins_and_a_removal_restores_the_one_before()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    let client = reader(&database.postgres).await?;
    database
        .writer
        .commit(
            &death(COMPOSITION, 1, Some("2026-06-15"))?,
            &RunId::new("run-1")?,
        )
        .await?;
    assert_eq!(vec![String::from("2026-06-15")], deaths(&client).await?);
    database
        .writer
        .commit(
            &death(OTHER_COMPOSITION, 1, Some("2026-06-16"))?,
            &RunId::new("run-1")?,
        )
        .await?;
    assert_eq!(
        vec![String::from("2026-06-16")],
        deaths(&client).await?,
        "one DEATH row per person, the newest writer's"
    );
    let other = watermark_of(&database.writer, OTHER_COMPOSITION).await?;
    database.writer.remove(&other).await?;
    assert_eq!(
        vec![String::from("2026-06-15")],
        deaths(&client).await?,
        "the writer that remains rewrites the row"
    );
    let first = watermark_of(&database.writer, COMPOSITION).await?;
    database.writer.remove(&first).await?;
    assert_eq!(
        Vec::<String>::new(),
        deaths(&client).await?,
        "no writer remains"
    );
    Ok(())
}

#[tokio::test]
async fn a_later_version_without_a_death_takes_its_row_with_it() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    let client = reader(&database.postgres).await?;
    database
        .writer
        .commit(
            &death(COMPOSITION, 1, Some("2026-06-15"))?,
            &RunId::new("run-1")?,
        )
        .await?;
    database
        .writer
        .commit(
            &death(COMPOSITION, 1, Some("2026-06-15"))?,
            &RunId::new("run-2")?,
        )
        .await?;
    assert_eq!(
        vec![String::from("2026-06-15")],
        deaths(&client).await?,
        "a re-run keeps one row"
    );
    database
        .writer
        .commit(&death(COMPOSITION, 2, None)?, &RunId::new("run-3")?)
        .await?;
    assert_eq!(Vec::<String>::new(), deaths(&client).await?);
    Ok(())
}
