// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Athena download loader: the file set and the format against the
//! vendored Athena source (`docs/specs/athena/`), and the load of the
//! synthetic download into a CDM schema the embedded DDL builds.
//!
//! The container-backed tests run only when `FERROBRIDGE_E2E=1` admits the
//! harness.

use ferrobridge_testkit::containers;
use ferrobridge_testkit::vocabulary;
use omop_cdm::athena::{self, Export, ExportError, LoadError};
use omop_cdm::database::{self, CdmPool};
use omop_cdm::ddl::SchemaName;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};

/// The vendored source of the Athena application's download writer.
fn athena_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/athena/src/main/java/com/odysseusinc/athena/service")
}

/// Returns the file name a vendored saver's `fileName()` returns.
fn saver_file(path: &Path) -> Result<String, Box<dyn Error>> {
    let text = std::fs::read_to_string(path)?;
    let after = text
        .split_once("fileName()")
        .ok_or("the saver names its file")?
        .1;
    let quoted = after.split('"').nth(1).ok_or("a quoted file name")?;
    Ok(quoted.to_owned())
}

#[test]
fn the_vendored_writer_states_the_shape_the_loader_reads() -> Result<(), Box<dyn Error>> {
    let source = athena_source();
    let values = std::fs::read_to_string(source.join("impl/ResultSetCsvValues.java"))?;
    assert!(
        values.contains("DATE_FORMAT = \"yyyyMMdd\""),
        "the date format"
    );
    let writer = std::fs::read_to_string(source.join("impl/AthenaCSVWriter.java"))?;
    assert!(writer.contains("LINE_END = \"\\n\""), "the line end");
    assert!(writer.contains("StandardCharsets.UTF_8"), "the encoding");
    assert!(
        writer.contains("line.append(separator)"),
        "the values are joined by the separator and never quoted"
    );
    Ok(())
}

#[test]
fn the_files_the_savers_name_are_the_download_the_loader_reads() -> Result<(), Box<dyn Error>> {
    let source = athena_source();
    let mut names = BTreeSet::new();
    for directory in ["saver/common", "saver/v5"] {
        for entry in std::fs::read_dir(source.join(directory))? {
            names.insert(saver_file(&entry?.path())?);
        }
    }
    let download = tempfile::tempdir()?;
    for name in &names {
        std::fs::write(download.path().join(name), "")?;
    }
    let export = Export::read(download.path())?;
    let loaded: BTreeSet<String> = export
        .files()
        .iter()
        .map(|file| format!("{}.csv", file.table().name.to_uppercase()))
        .collect();
    let ignored: BTreeSet<String> = export.ignored().iter().cloned().collect();
    assert_eq!(9, loaded.len(), "the nine served tables: {loaded:?}");
    assert_eq!(
        names,
        loaded.union(&ignored).cloned().collect(),
        "every file a saver writes is loaded or named as left alone"
    );
    assert_eq!(
        BTreeSet::from(
            [
                "CONCEPT_CPT4.csv",
                "CONCEPT_METADATA.csv",
                "CONCEPT_RELATIONSHIP_METADATA.csv",
                "PACK_CONTENT.csv",
                "cpt4_ref.csv",
            ]
            .map(String::from)
        ),
        ignored
    );
    Ok(())
}

#[test]
fn a_directory_without_a_served_file_is_refused_naming_each() -> Result<(), Box<dyn Error>> {
    let download = tempfile::tempdir()?;
    std::fs::write(download.path().join("CONCEPT.csv"), "concept_id\n")?;
    std::fs::write(download.path().join("concept_class.csv"), "")?;
    let refused = Export::read(download.path()).expect_err("the download is incomplete");
    let ExportError::Missing { missing, .. } = refused else {
        panic!("{refused:?}");
    };
    assert_eq!(8, missing.len(), "{missing:?}");
    assert!(
        missing.contains(&String::from("CONCEPT_CLASS.csv")),
        "a file is taken by its exact name"
    );
    assert!(!missing.contains(&String::from("SOURCE_TO_CONCEPT_MAP.csv")));
    Ok(())
}

#[test]
fn an_unreadable_directory_is_refused() {
    let refused = Export::read(Path::new("/nonexistent/ferrobridge/athena"));
    assert!(matches!(refused, Err(ExportError::Directory { .. })));
}

/// Starts PostgreSQL and builds the CDM schema, or returns `None` when the
/// harness is not admitted.
async fn schema() -> Result<Option<(containers::Postgres, CdmPool)>, Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(None);
    }
    let postgres = containers::postgres().await?;
    let options: PgConnectOptions = postgres.url().parse()?;
    let pool = CdmPool::connect(
        PgPoolOptions::new().max_connections(2),
        options,
        SchemaName::new("cdm")?,
    )
    .await?;
    database::init(&pool).await?;
    Ok(Some((postgres, pool)))
}

/// Returns the rows each fixture file holds below its header.
fn fixture_rows() -> Result<Vec<(String, u64)>, Box<dyn Error>> {
    let mut rows = Vec::new();
    for entry in std::fs::read_dir(vocabulary::directory())? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some(table) = name.strip_suffix(".csv") {
            let lines = std::fs::read_to_string(&path)?.lines().count();
            rows.push((
                table.to_lowercase(),
                u64::try_from(lines.saturating_sub(1))?,
            ));
        }
    }
    rows.sort();
    Ok(rows)
}

#[tokio::test]
async fn the_synthetic_download_loads_every_row_and_names_its_version() -> Result<(), Box<dyn Error>>
{
    let Some((_postgres, pool)) = schema().await? else {
        return Ok(());
    };
    let export = Export::read(&vocabulary::directory())?;
    assert_eq!(vec![String::from("README.md")], export.ignored());
    let loaded = athena::load(&pool, &export).await?;
    let mut written: Vec<(String, u64)> = loaded
        .tables()
        .iter()
        .map(|(table, rows)| ((*table).to_owned(), *rows))
        .collect();
    written.sort();
    assert_eq!(fixture_rows()?, written);
    assert_eq!(Some("v5.0 synthetic-1"), loaded.vocabulary_version());
    let date: String = sqlx::query_scalar(
        "SELECT to_char(valid_end_date, 'YYYY-MM-DD') FROM concept WHERE concept_id = 1001",
    )
    .fetch_one(pool.pool())
    .await?;
    assert_eq!(
        "2099-12-31", date,
        "a yyyyMMdd date reads as the date it names"
    );
    let synonym: String = sqlx::query_scalar(
        "SELECT concept_synonym_name FROM concept_synonym WHERE concept_id = 1001",
    )
    .fetch_one(pool.pool())
    .await?;
    assert_eq!(
        "Synthetic measurement, another name", synonym,
        "a comma is text"
    );
    Ok(())
}

#[tokio::test]
async fn a_second_load_replaces_the_rows() -> Result<(), Box<dyn Error>> {
    let Some((_postgres, pool)) = schema().await? else {
        return Ok(());
    };
    let export = Export::read(&vocabulary::directory())?;
    let first = athena::load(&pool, &export).await?;
    let second = athena::load(&pool, &export).await?;
    assert_eq!(first, second);
    let concepts: i64 = sqlx::query_scalar("SELECT count(*) FROM concept")
        .fetch_one(pool.pool())
        .await?;
    let expected = fixture_rows()?
        .into_iter()
        .find(|(table, _)| table == "concept")
        .map(|(_, rows)| rows)
        .ok_or("a concept fixture")?;
    assert_eq!(expected, u64::try_from(concepts)?, "nothing is appended");
    Ok(())
}

/// Copies the synthetic download into a directory of its own, with
/// `CONCEPT.csv` replaced by `concept`.
fn download_with(concept: &str) -> Result<tempfile::TempDir, Box<dyn Error>> {
    let download = tempfile::tempdir()?;
    for entry in std::fs::read_dir(vocabulary::directory())? {
        let path = entry?.path();
        if let Some(name) = path.file_name() {
            std::fs::copy(&path, download.path().join(name))?;
        }
    }
    std::fs::write(download.path().join("CONCEPT.csv"), concept)?;
    Ok(download)
}

#[tokio::test]
async fn a_file_holding_the_quote_byte_is_refused_and_its_table_keeps_its_rows()
-> Result<(), Box<dyn Error>> {
    let Some((_postgres, pool)) = schema().await? else {
        return Ok(());
    };
    athena::load(&pool, &Export::read(&vocabulary::directory())?).await?;
    let header = "concept_id\tconcept_name\tdomain_id\tvocabulary_id\tconcept_class_id\t\
                  standard_concept\tconcept_code\tvalid_start_date\tvalid_end_date\tinvalid_reason\n";
    let download = download_with(&format!(
        "{header}7001\tSynthetic\u{8}name\tMetadata\tFB-META\tFB-META\t\tX\t19700101\t20991231\t\n"
    ))?;
    let refused = athena::load(&pool, &Export::read(download.path())?)
        .await
        .expect_err("a backspace byte is refused");
    assert!(matches!(refused, LoadError::Quote { .. }), "{refused:?}");
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM concept WHERE concept_id = 1001")
        .fetch_one(pool.pool())
        .await?;
    assert_eq!(1, kept, "the refused table keeps the rows it held");
    Ok(())
}

#[tokio::test]
async fn a_header_naming_no_column_of_the_table_is_refused() -> Result<(), Box<dyn Error>> {
    let Some((_postgres, pool)) = schema().await? else {
        return Ok(());
    };
    let download = download_with("concept_id\tconcept_colour\n1\tred\n")?;
    let refused = athena::load(&pool, &Export::read(download.path())?)
        .await
        .expect_err("the header is refused");
    assert!(
        matches!(&refused, LoadError::Column { column, .. } if column == "concept_colour"),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_row_whose_field_count_disagrees_with_the_header_is_refused() -> Result<(), Box<dyn Error>>
{
    let Some((_postgres, pool)) = schema().await? else {
        return Ok(());
    };
    let download = download_with(
        "concept_id\tconcept_name\tdomain_id\tvocabulary_id\tconcept_class_id\t\
         standard_concept\tconcept_code\tvalid_start_date\tvalid_end_date\tinvalid_reason\n\
         7001\tSplit\n",
    )?;
    let refused = athena::load(&pool, &Export::read(download.path())?)
        .await
        .expect_err("a short row is refused");
    assert!(
        matches!(
            refused,
            LoadError::Refused {
                table: "concept",
                ..
            }
        ),
        "{refused:?}"
    );
    Ok(())
}
