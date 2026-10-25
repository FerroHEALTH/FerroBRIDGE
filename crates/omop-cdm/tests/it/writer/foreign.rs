// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The survey of the rows another ETL loaded into a table the writer assigns
//! the ids of. No specification governs key assignment: FerroBRIDGE's own
//! design.

use ferrobridge_testkit::containers;
use omop_cdm::writer::{foreign::ForeignRows, input::PersonPolicy, input::RunId};
use std::error::Error;

use super::{database, laboratory, reader};

#[tokio::test]
async fn the_rows_the_writer_wrote_are_all_owned() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    assert_eq!(
        Vec::<ForeignRows>::new(),
        database.writer.foreign_rows().await?
    );
    database
        .writer
        .commit(&laboratory(1, 2)?, &RunId::new("run-1")?)
        .await?;
    assert_eq!(
        Vec::<ForeignRows>::new(),
        database.writer.foreign_rows().await?
    );
    Ok(())
}

#[tokio::test]
async fn a_row_another_load_wrote_is_counted_by_table() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut database = database(PersonPolicy::CreateOnFirstSight).await?;
    database
        .writer
        .commit(&laboratory(1, 1)?, &RunId::new("run-1")?)
        .await?;
    let client = reader(&database.postgres).await?;
    client
        .batch_execute(
            "INSERT INTO cdm.person (person_id, gender_concept_id, year_of_birth,
                 race_concept_id, ethnicity_concept_id)
             VALUES (900, 0, 1970, 0, 0);
             INSERT INTO cdm.measurement (measurement_id, person_id, measurement_concept_id,
                 measurement_date, measurement_type_concept_id)
             VALUES (901, 900, 0, '2026-06-15', 32817), (902, 900, 0, '2026-06-15', 32817)",
        )
        .await?;
    assert_eq!(
        vec![
            ForeignRows {
                table: "person",
                rows: 1
            },
            ForeignRows {
                table: "measurement",
                rows: 2
            },
        ],
        database.writer.foreign_rows().await?
    );
    Ok(())
}
