// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The lineage of a composition mapped from a FHIR resource: every row it
//! wrote names the resource, and the resource names every row. No
//! specification governs lineage storage: FerroBRIDGE's own design.

use ferrobridge_testkit::containers;
use omop_cdm::writer::input::{PersonPolicy, RunId};
use omop_cdm::writer::origin::{ResourceOrigin, WrittenRow};
use omop_cdm::writer::removal::Removal;
use std::error::Error;

use super::{database, laboratory};

/// The synthetic Observation the laboratory composition was mapped from.
fn observation(version: &str) -> ResourceOrigin {
    ResourceOrigin {
        resource_type: String::from("Observation"),
        id: String::from("lab-1"),
        version: Some(version.to_owned()),
    }
}

#[tokio::test]
async fn a_row_names_its_resource_and_the_resource_names_its_rows() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    database
        .writer
        .commit_from(
            &laboratory(1, 1)?,
            &RunId::new("run-1")?,
            Some(&observation("1")),
        )
        .await?;
    assert_eq!(
        vec![observation("1")],
        database.writer.origins_of_row("measurement", 2).await?
    );
    assert_eq!(
        vec![
            WrittenRow {
                table: String::from("measurement"),
                id: 1
            },
            WrittenRow {
                table: String::from("measurement"),
                id: 2
            },
        ],
        database
            .writer
            .rows_of_resource("Observation", "lab-1")
            .await?
    );
    database
        .writer
        .commit_from(
            &laboratory(2, 1)?,
            &RunId::new("run-2")?,
            Some(&observation("2")),
        )
        .await?;
    assert_eq!(
        vec![observation("2")],
        database.writer.origins_of_row("measurement", 2).await?,
        "a later version records its own"
    );
    Ok(())
}

#[tokio::test]
async fn a_composition_from_a_cdr_has_no_resource_and_a_removal_forgets_one()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    database
        .writer
        .commit(&laboratory(1, 1)?, &RunId::new("run-1")?)
        .await?;
    assert_eq!(
        Vec::<ResourceOrigin>::new(),
        database.writer.origins_of_row("measurement", 2).await?
    );
    database
        .writer
        .commit_from(
            &laboratory(2, 1)?,
            &RunId::new("run-2")?,
            Some(&observation("2")),
        )
        .await?;
    let committed = database
        .writer
        .watermarks_after(None, 10)
        .await?
        .pop()
        .ok_or("a watermark")?;
    let removal = database.writer.remove(&committed).await?;
    assert!(matches!(removal, Removal::Removed(_)), "{removal:?}");
    assert_eq!(
        Vec::<WrittenRow>::new(),
        database
            .writer
            .rows_of_resource("Observation", "lab-1")
            .await?
    );
    Ok(())
}
