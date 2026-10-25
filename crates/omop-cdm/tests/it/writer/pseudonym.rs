// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pseudonymised write: keyed pseudonyms for the columns that link a row
//! back to its source, NULL for the columns that name a provider, and one date
//! shift per person. No specification governs pseudonymisation: FerroBRIDGE's
//! own design over the CDM field definitions.

use ferrobridge_testkit::containers;
use omop_cdm::database::CdmPool;
use omop_cdm::graph::{
    RecordGraph, key::Visit, key::VisitKey, key::VisitSource, row::Reference, row::Row, row::Value,
};
use omop_cdm::value::CdmDate;
use omop_cdm::writer::input::{PersonPolicy, RunId, VisitConcepts};
use omop_cdm::writer::pseudonym::{
    MAX_SHIFT_DAYS, MIN_KEY_BYTES, PseudonymError, Pseudonymiser, Treatment, treatment,
};
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use std::error::Error;

use super::shared::drug_exposure;
use super::{Database, EHR, analyte, database, laboratory, reader, source};

/// A synthetic 32-byte key.
const KEY: &[u8; 32] = b"ferrobridge-synthetic-test-key-1";

/// Another synthetic 32-byte key.
const OTHER_KEY: &[u8; 32] = b"ferrobridge-synthetic-test-key-2";

/// The source of the synthetic visit.
const VISIT: &str = "encounter-pseudonym-1";

/// The source of the synthetic provider.
const PROVIDER: &str = "practitioner-pseudonym-1";

#[test]
fn a_pseudonym_is_forty_hex_characters_stable_under_one_key() -> Result<(), Box<dyn Error>> {
    let first = Pseudonymiser::new(KEY, 0)?;
    let again = Pseudonymiser::new(KEY, 0)?;
    let pseudonym = first.pseudonym(EHR);
    assert_eq!(40, pseudonym.len());
    assert!(
        pseudonym
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert_eq!(pseudonym, again.pseudonym(EHR), "one key, one pseudonym");
    assert_ne!(
        pseudonym,
        first.pseudonym(VISIT),
        "two values, two pseudonyms"
    );
    assert_ne!(
        pseudonym,
        Pseudonymiser::new(OTHER_KEY, 0)?.pseudonym(EHR),
        "two keys, two pseudonyms"
    );
    Ok(())
}

#[test]
fn a_short_key_and_a_long_shift_are_refused() {
    assert_eq!(
        Some(PseudonymError::ShortKey { length: 31 }),
        Pseudonymiser::new(&[7; MIN_KEY_BYTES - 1], 0).err()
    );
    assert_eq!(
        Some(PseudonymError::ShiftTooLarge {
            days: MAX_SHIFT_DAYS + 1
        }),
        Pseudonymiser::new(KEY, MAX_SHIFT_DAYS + 1).err()
    );
}

#[test]
fn a_shift_stays_within_its_bound_and_is_one_per_person() -> Result<(), Box<dyn Error>> {
    assert_eq!(0, Pseudonymiser::new(KEY, 0)?.shift(EHR), "no shift asked");
    let pseudonymiser = Pseudonymiser::new(KEY, 30)?;
    let shifts: Vec<i32> = (0..200)
        .map(|person| pseudonymiser.shift(&format!("ehr-{person}")))
        .collect();
    assert!(
        shifts.iter().all(|shift| (-30..=30).contains(shift)),
        "{shifts:?}"
    );
    assert!(shifts.iter().any(|shift| *shift < 0) && shifts.iter().any(|shift| *shift > 0));
    assert_eq!(pseudonymiser.shift(EHR), pseudonymiser.shift(EHR));
    Ok(())
}

#[test]
fn the_treatment_follows_the_cdm_field_definitions() {
    assert_eq!(
        Some(Treatment::Pseudonym),
        treatment("person", "person_source_value")
    );
    assert_eq!(
        Some(Treatment::Pseudonym),
        treatment("visit_occurrence", "visit_source_value")
    );
    assert_eq!(
        Some(Treatment::Erase),
        treatment("provider", "provider_name")
    );
    assert_eq!(Some(Treatment::Erase), treatment("provider", "npi"));
    assert_eq!(None, treatment("measurement", "measurement_source_value"));
    assert_eq!(None, treatment("person", "gender_source_value"));
}

/// A graph of version 1 with one measurement, one drug exposure naming the
/// synthetic provider, and a `PERSON` row carrying the EHR as its source.
fn graph(visit: &VisitKey) -> Result<RecordGraph, Box<dyn Error>> {
    let mut graph = laboratory(1, 1)?;
    graph.push_row(drug_exposure(analyte(7)?, PROVIDER, None)?)?;
    graph.push_row(
        Row::builder("person", analyte(8)?)?
            .value("gender_concept_id", Value::Integer(0))?
            .value("year_of_birth", Value::Integer(1970))?
            .value("month_of_birth", Value::Integer(12))?
            .value("day_of_birth", Value::Integer(31))?
            .value("race_concept_id", Value::Integer(0))?
            .value("ethnicity_concept_id", Value::Integer(0))?
            .value("person_source_value", Value::Text(EHR.to_owned()))?
            .build()?,
    )?;
    graph.push_row(
        Row::builder("measurement", analyte(9)?)?
            .reference("person_id", Reference::Person(HierObjectId::new(EHR)?))?
            .value("measurement_concept_id", Value::Integer(1001))?
            .value("measurement_date", Value::Date(CdmDate::new("2026-06-15")?))?
            .value("measurement_type_concept_id", Value::Integer(32817))?
            .reference("visit_occurrence_id", Reference::Visit(visit.clone()))?
            .build()?,
    )?;
    Ok(graph)
}

/// Writes the synthetic visit and graph through a writer pseudonymised under
/// `key` with a 30-day shift.
async fn write(key: &[u8]) -> Result<Database, Box<dyn Error>> {
    let Database { postgres, writer } = database(PersonPolicy::CreateOnFirstSight).await?;
    let mut writer = writer.pseudonymise(Pseudonymiser::new(key, 30)?);
    let visit = VisitKey::new(HierObjectId::new(EHR)?, VisitSource::new(VISIT)?);
    writer
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
    writer
        .commit(&graph(&visit)?, &RunId::new("run-1")?)
        .await?;
    Ok(Database { postgres, writer })
}

#[tokio::test]
async fn no_source_identifier_reaches_a_text_column_of_the_cdm() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let database = write(KEY).await?;
    let client = reader(&database.postgres).await?;
    let columns = client
        .query(
            "SELECT table_name::text, column_name::text FROM information_schema.columns
             WHERE table_schema = 'cdm' AND data_type IN ('character varying', 'text')
             ORDER BY table_name, column_name",
            &[],
        )
        .await?;
    assert!(!columns.is_empty(), "the CDM has text columns");
    let composition = source(1)?;
    let identifiers = [
        EHR,
        VISIT,
        PROVIDER,
        composition.versioned_object_uid().value(),
    ];
    let mut found = Vec::new();
    for column in &columns {
        let (table, name): (&str, &str) = (column.get(0), column.get(1));
        for identifier in identifiers {
            let hits: i64 = client
                .query_one(
                    &format!(
                        "SELECT count(*) FROM cdm.\"{table}\" WHERE strpos(\"{name}\", $1) > 0"
                    ),
                    &[&identifier],
                )
                .await?
                .get(0);
            if hits > 0 {
                found.push(format!("{table}.{name} holds {identifier}"));
            }
        }
    }
    assert_eq!(Vec::<String>::new(), found);
    let pseudonym = Pseudonymiser::new(KEY, 0)?.pseudonym(EHR);
    let person: Option<String> = client
        .query_one("SELECT person_source_value FROM cdm.person", &[])
        .await?
        .get(0);
    assert_eq!(Some(pseudonym), person, "the pseudonym joins across runs");
    let provider: (Option<String>, Option<String>) = client
        .query_one(
            "SELECT provider_source_value, provider_name FROM cdm.provider",
            &[],
        )
        .await
        .map(|row| (row.get(0), row.get(1)))?;
    assert_eq!(
        (Some(Pseudonymiser::new(KEY, 0)?.pseudonym(PROVIDER)), None),
        provider
    );
    Ok(())
}

#[tokio::test]
async fn every_date_of_a_person_moves_by_one_shift() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let database = write(KEY).await?;
    let client = reader(&database.postgres).await?;
    let shift = Pseudonymiser::new(KEY, 30)?.shift(EHR);
    let moved = |date: &str| format!("SELECT ('{date}'::date + {shift})::text");
    let expected_visit: String = client.query_one(&moved("2026-06-14"), &[]).await?.get(0);
    let expected_measurement: String = client.query_one(&moved("2026-06-15"), &[]).await?.get(0);
    let expected_birth: String = client.query_one(&moved("1970-12-31"), &[]).await?.get(0);
    let row = client
        .query_one(
            "SELECT visit_start_date::text, (visit_end_date - visit_start_date)
             FROM cdm.visit_occurrence",
            &[],
        )
        .await?;
    assert_eq!(expected_visit, row.get::<_, String>(0));
    assert_eq!(2, row.get::<_, i32>(1), "the visit keeps its length");
    let measurements: Vec<String> = client
        .query(
            "SELECT DISTINCT measurement_date::text FROM cdm.measurement",
            &[],
        )
        .await?
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert_eq!(vec![expected_measurement], measurements);
    let birth: String = client
        .query_one(
            "SELECT make_date(year_of_birth, month_of_birth, day_of_birth)::text FROM cdm.person",
            &[],
        )
        .await?
        .get(0);
    assert_eq!(
        expected_birth, birth,
        "the birth date moves with the person"
    );
    Ok(())
}

#[tokio::test]
async fn another_key_gives_other_pseudonyms() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let database = write(OTHER_KEY).await?;
    let client = reader(&database.postgres).await?;
    let visit: Option<String> = client
        .query_one("SELECT visit_source_value FROM cdm.visit_occurrence", &[])
        .await?
        .get(0);
    assert_eq!(
        Some(Pseudonymiser::new(OTHER_KEY, 0)?.pseudonym(VISIT)),
        visit
    );
    assert_ne!(Some(Pseudonymiser::new(KEY, 0)?.pseudonym(VISIT)), visit);
    Ok(())
}

#[tokio::test]
async fn a_shifted_person_passes_the_temporal_plausibility_checks() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    assert_ne!(
        0,
        Pseudonymiser::new(KEY, 30)?.shift(EHR),
        "the synthetic person moves"
    );
    let database = write(KEY).await?;
    let pool = CdmPool::connect(
        sqlx::postgres::PgPoolOptions::new().max_connections(1),
        database.postgres.url().parse()?,
        omop_cdm::ddl::SchemaName::new("cdm")?,
    )
    .await?;
    let plan =
        omop_cdm::quality::plan::Plan::new(&omop_cdm::quality::plan::PlanOptions::default())?;
    let results = omop_cdm::quality::run::run(&pool, &plan).await;
    let temporal: Vec<_> = results
        .iter()
        .filter(|result| {
            result.category == omop_cdm::quality::definitions::KahnCategory::Plausibility
        })
        .collect();
    assert!(
        temporal.iter().any(|result| result.passed),
        "a temporal check ran over the shifted rows"
    );
    let failing: Vec<&str> = temporal
        .iter()
        .filter(|result| result.failed || result.is_error)
        .map(|result| result.check_id.as_str())
        .collect();
    assert_eq!(Vec::<&str>::new(), failing, "the intervals stay intact");
    Ok(())
}
