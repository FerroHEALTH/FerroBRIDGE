// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The removal of a composition's rows and the watermark pages.

use ferrobridge_testkit::containers;
use omop_cdm::writer::{
    CdmWriter, input::PersonPolicy, input::RunId, removal::Committed, removal::Removal,
};
use std::error::Error;

use super::{COMPOSITION, EHR, count, database, laboratory, reader};

/// Returns the one watermark the writer holds.
async fn only_watermark(writer: &CdmWriter) -> Result<Committed, Box<dyn Error>> {
    let mut committed = writer.watermarks_after(None, 10).await?;
    assert_eq!(1, committed.len(), "one composition was committed");
    Ok(committed.remove(0))
}

#[tokio::test]
async fn a_removal_deletes_the_rows_links_and_watermark_and_keeps_the_person()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    database
        .writer
        .commit(&laboratory(2, 2)?, &RunId::new("run-1")?)
        .await?;
    let committed = only_watermark(&database.writer).await?;
    assert_eq!(COMPOSITION, committed.versioned_object_uid().value());
    assert_eq!(EHR, committed.ehr_id().value());
    assert_eq!(
        format!("{COMPOSITION}::ferrobridge.test::2"),
        committed.version_uid().value()
    );

    let removal = database.writer.remove(&committed).await?;
    let Removal::Removed(rows) = removal else {
        return Err(format!("expected a removal, got {removal:?}").into());
    };
    assert_eq!(Some(&3), rows.get("measurement"));
    assert_eq!(Some(&4), rows.get("fact_relationship"));
    let client = reader(&database.postgres).await?;
    for table in [
        "cdm.measurement",
        "cdm.fact_relationship",
        "ferrobridge.record",
        "ferrobridge.link",
        "ferrobridge.watermark",
    ] {
        assert_eq!(0, count(&client, table).await?, "{table} is empty");
    }
    assert_eq!(
        1,
        count(&client, "ferrobridge.person_map").await?,
        "the EHR keeps its person_id"
    );

    assert_eq!(
        Removal::Superseded,
        database.writer.remove(&committed).await?,
        "a second removal finds no watermark"
    );
    Ok(())
}

#[tokio::test]
async fn a_removal_overtaken_by_a_newer_version_deletes_nothing() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    database
        .writer
        .commit(&laboratory(1, 2)?, &RunId::new("run-1")?)
        .await?;
    let stale = only_watermark(&database.writer).await?;
    database
        .writer
        .commit(&laboratory(2, 2)?, &RunId::new("run-2")?)
        .await?;
    assert_eq!(Removal::Superseded, database.writer.remove(&stale).await?);
    let client = reader(&database.postgres).await?;
    assert_eq!(3, count(&client, "cdm.measurement").await?);
    assert_eq!(1, count(&client, "ferrobridge.watermark").await?);
    Ok(())
}

#[tokio::test]
async fn the_watermarks_page_in_order_of_the_versioned_composition() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    database
        .writer
        .commit(&laboratory(1, 1)?, &RunId::new("run-1")?)
        .await?;
    let first = database.writer.watermarks_after(None, 1).await?;
    assert_eq!(1, first.len());
    let last = first.last().ok_or("one watermark")?;
    assert!(
        database
            .writer
            .watermarks_after(Some(last.versioned_object_uid()), 1)
            .await?
            .is_empty(),
        "nothing follows the last watermark"
    );
    Ok(())
}
