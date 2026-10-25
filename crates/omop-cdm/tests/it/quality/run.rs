// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The nine conformance checks against PostgreSQL, each on a database seeded
//! to pass it and on one seeded to violate it, and the whole default plan on
//! an empty CDM.
//!
//! The container-backed tests run only when `FERROBRIDGE_E2E=1` admits the
//! harness. The DDL declares the primary keys and the NOT NULL columns, so the
//! violating seeds for `isRequired`, `isPrimaryKey` and `cdmDatatype` alter
//! the schema first. Every seed is synthetic.

use std::error::Error;

use ferrobridge_testkit::containers::{self, Postgres};
use omop_cdm::database::{self, CdmPool};
use omop_cdm::ddl::SchemaName;
use omop_cdm::quality::check::CheckName;
use omop_cdm::quality::evaluate::{CheckResult, Counts};
use omop_cdm::quality::plan::{Plan, PlanOptions};
use omop_cdm::quality::run;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use super::{find, planned};

/// The schema the tests build the CDM in.
const SCHEMA: &str = "cdm";

/// A container and a pool on an initialised CDM in it; the container lives as
/// long as the pool is used.
struct Cdm {
    _postgres: Postgres,
    pool: CdmPool,
}

/// Starts PostgreSQL and initialises the CDM, or returns `None` when the
/// end-to-end harness is not admitted.
async fn cdm() -> Result<Option<Cdm>, Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(None);
    }
    let postgres = containers::postgres().await?;
    let options: PgConnectOptions = postgres.url().parse()?;
    let pool = CdmPool::connect(
        PgPoolOptions::new().max_connections(2),
        options,
        SchemaName::new(SCHEMA)?,
    )
    .await?;
    database::init(&pool).await?;
    Ok(Some(Cdm {
        _postgres: postgres,
        pool,
    }))
}

impl Cdm {
    /// Runs `sql` on the CDM schema.
    async fn execute(&self, sql: &str) -> Result<(), Box<dyn Error>> {
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(self.pool.pool())
            .await?;
        Ok(())
    }

    /// Runs the default plan's `name` check on `table` or `field`.
    async fn check(
        &self,
        name: CheckName,
        table: &str,
        field: Option<&str>,
    ) -> Result<CheckResult, Box<dyn Error>> {
        Ok(run::run_check(&self.pool, &planned(name, table, field)?).await)
    }
}

/// The counts `violated` of `denominator`.
fn counts(violated: u64, denominator: u64) -> Counts {
    Counts {
        violated,
        denominator,
    }
}

/// Asserts a result passed with `expected` counts.
fn assert_passed(result: &CheckResult, expected: Counts) {
    assert_eq!(Some(expected), result.counts, "{:?}", result.error);
    assert!(
        result.passed,
        "{} did not pass: {result:?}",
        result.check_id
    );
    assert!(!result.failed, "a passed result is not failed");
    assert!(!result.is_error, "a passed result is not in error");
}

/// Asserts a result failed with `expected` counts.
fn assert_failed(result: &CheckResult, expected: Option<Counts>) {
    assert_eq!(expected, result.counts, "{:?}", result.error);
    assert!(
        result.failed,
        "{} did not fail: {result:?}",
        result.check_id
    );
    assert!(!result.passed, "a failed result is not passed");
    assert!(!result.is_error, "a failed result is not in error");
}

/// Two synthetic persons; the concept ids are the tests' own.
const PERSONS: &str = "INSERT INTO person \
    (person_id, gender_concept_id, year_of_birth, race_concept_id, ethnicity_concept_id) \
    VALUES (1, 9001, 1980, 0, 0), (2, 9001, 1990, 0, 0)";

/// Synthetic concepts: 9001 a standard, valid Gender concept, 9002 a
/// Condition concept, 9003 a Gender concept that is not standard, 9101 an
/// Ingredient and 9102 a Clinical Drug.
const CONCEPTS: &str = "INSERT INTO concept (concept_id, concept_name, domain_id, vocabulary_id, \
    concept_class_id, standard_concept, concept_code, valid_start_date, valid_end_date, \
    invalid_reason) VALUES \
    (9001, 'Test gender', 'Gender', 'Test', 'Gender', 'S', 'G1', '1970-01-01', '2099-12-31', NULL), \
    (9002, 'Test condition', 'Condition', 'Test', 'Clinical Finding', 'S', 'C1', '1970-01-01', '2099-12-31', NULL), \
    (9003, 'Test retired gender', 'Gender', 'Test', 'Gender', NULL, 'G2', '1970-01-01', '2099-12-31', 'D'), \
    (9101, 'Test ingredient', 'Drug', 'Test', 'Ingredient', 'S', 'I1', '1970-01-01', '2099-12-31', NULL), \
    (9102, 'Test clinical drug', 'Drug', 'Test', 'Clinical Drug', 'S', 'D1', '1970-01-01', '2099-12-31', NULL)";

#[tokio::test]
async fn every_planned_query_runs_and_none_fails_on_an_empty_cdm() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    let plan = Plan::new(&PlanOptions::default())?;
    for result in run::run(&cdm.pool, &plan).await {
        assert!(
            !result.failed && !result.is_error,
            "{} failed or erred on an empty CDM: {:?}",
            result.check_id,
            result.error
        );
    }
    Ok(())
}

#[tokio::test]
async fn cdm_table_passes_on_a_present_table() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    let result = cdm.check(CheckName::CdmTable, "person", None).await?;
    assert_passed(&result, counts(0, 1));
    Ok(())
}

#[tokio::test]
async fn cdm_table_fails_without_error_on_a_dropped_table() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute("DROP TABLE person").await?;
    let result = cdm.check(CheckName::CdmTable, "person", None).await?;
    assert_failed(&result, None);
    assert!(
        result.error.is_some(),
        "the database's error stays on the result"
    );
    Ok(())
}

#[tokio::test]
async fn cdm_field_passes_on_a_present_column() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    let result = cdm
        .check(CheckName::CdmField, "note_nlp", Some("offset"))
        .await?;
    assert_passed(&result, counts(0, 1));
    Ok(())
}

#[tokio::test]
async fn cdm_field_fails_without_error_on_a_dropped_column() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute("ALTER TABLE person DROP COLUMN month_of_birth")
        .await?;
    let result = cdm
        .check(CheckName::CdmField, "person", Some("month_of_birth"))
        .await?;
    assert_failed(&result, None);
    Ok(())
}

#[tokio::test]
async fn a_counting_check_on_a_dropped_table_is_an_error() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute("DROP TABLE person").await?;
    let result = cdm
        .check(CheckName::IsRequired, "person", Some("year_of_birth"))
        .await?;
    assert!(result.is_error);
    assert!(!result.passed);
    assert!(!result.failed);
    assert_eq!(None, result.counts);
    Ok(())
}

#[tokio::test]
async fn is_required_passes_when_every_row_has_a_value() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    let result = cdm
        .check(CheckName::IsRequired, "person", Some("year_of_birth"))
        .await?;
    assert_passed(&result, counts(0, 2));
    Ok(())
}

#[tokio::test]
async fn is_required_fails_on_a_null() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute("ALTER TABLE person ALTER COLUMN year_of_birth DROP NOT NULL")
        .await?;
    cdm.execute(PERSONS).await?;
    cdm.execute(
        "INSERT INTO person (person_id, gender_concept_id, year_of_birth, race_concept_id, \
         ethnicity_concept_id) VALUES (3, 9001, NULL, 0, 0)",
    )
    .await?;
    let result = cdm
        .check(CheckName::IsRequired, "person", Some("year_of_birth"))
        .await?;
    assert_failed(&result, Some(counts(1, 3)));
    Ok(())
}

#[tokio::test]
async fn cdm_datatype_passes_on_integers() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    let result = cdm
        .check(CheckName::CdmDatatype, "person", Some("year_of_birth"))
        .await?;
    assert_passed(&result, counts(0, 2));
    Ok(())
}

#[tokio::test]
async fn cdm_datatype_fails_on_a_decimal_and_text() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute("ALTER TABLE person ALTER COLUMN year_of_birth TYPE varchar(20)")
        .await?;
    cdm.execute(
        "INSERT INTO person (person_id, gender_concept_id, year_of_birth, race_concept_id, \
         ethnicity_concept_id) VALUES (1, 9001, '1980', 0, 0), (2, 9001, '-12', 0, 0), \
         (3, 9001, '1980.5', 0, 0), (4, 9001, 'nineteen', 0, 0)",
    )
    .await?;
    let result = cdm
        .check(CheckName::CdmDatatype, "person", Some("year_of_birth"))
        .await?;
    assert_failed(&result, Some(counts(2, 4)));
    Ok(())
}

#[tokio::test]
async fn is_primary_key_passes_on_distinct_values() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    let result = cdm
        .check(CheckName::IsPrimaryKey, "person", Some("person_id"))
        .await?;
    assert_passed(&result, counts(0, 2));
    Ok(())
}

#[tokio::test]
async fn is_primary_key_fails_on_every_row_of_a_duplicate() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute("ALTER TABLE person DROP CONSTRAINT xpk_person")
        .await?;
    cdm.execute(PERSONS).await?;
    cdm.execute(
        "INSERT INTO person (person_id, gender_concept_id, year_of_birth, race_concept_id, \
         ethnicity_concept_id) VALUES (1, 9001, 2000, 0, 0)",
    )
    .await?;
    let result = cdm
        .check(CheckName::IsPrimaryKey, "person", Some("person_id"))
        .await?;
    assert_failed(&result, Some(counts(2, 3)));
    Ok(())
}

#[tokio::test]
async fn is_foreign_key_passes_on_a_present_row_and_a_null() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute("INSERT INTO care_site (care_site_id) VALUES (7)")
        .await?;
    cdm.execute(PERSONS).await?;
    cdm.execute("UPDATE person SET care_site_id = 7 WHERE person_id = 1")
        .await?;
    let result = cdm
        .check(CheckName::IsForeignKey, "person", Some("care_site_id"))
        .await?;
    assert_passed(&result, counts(0, 2));
    Ok(())
}

#[tokio::test]
async fn is_foreign_key_fails_on_a_missing_row() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute("UPDATE person SET care_site_id = 99 WHERE person_id = 1")
        .await?;
    let result = cdm
        .check(CheckName::IsForeignKey, "person", Some("care_site_id"))
        .await?;
    assert_failed(&result, Some(counts(1, 2)));
    Ok(())
}

#[tokio::test]
async fn fk_domain_passes_on_the_named_domain_and_concept_zero() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(CONCEPTS).await?;
    cdm.execute(PERSONS).await?;
    cdm.execute("UPDATE person SET gender_concept_id = 0 WHERE person_id = 2")
        .await?;
    let result = cdm
        .check(CheckName::FkDomain, "person", Some("gender_concept_id"))
        .await?;
    assert_passed(&result, counts(0, 2));
    Ok(())
}

#[tokio::test]
async fn fk_domain_fails_on_another_domain() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(CONCEPTS).await?;
    cdm.execute(PERSONS).await?;
    cdm.execute("UPDATE person SET gender_concept_id = 9002 WHERE person_id = 2")
        .await?;
    let result = cdm
        .check(CheckName::FkDomain, "person", Some("gender_concept_id"))
        .await?;
    assert_failed(&result, Some(counts(1, 2)));
    Ok(())
}

#[tokio::test]
async fn fk_class_passes_on_the_named_class() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(CONCEPTS).await?;
    cdm.execute(
        "INSERT INTO drug_era (drug_era_id, person_id, drug_concept_id, drug_era_start_date, \
         drug_era_end_date) VALUES (1, 1, 9101, '2020-01-01', '2020-02-01')",
    )
    .await?;
    let result = cdm
        .check(CheckName::FkClass, "drug_era", Some("drug_concept_id"))
        .await?;
    assert_passed(&result, counts(0, 1));
    Ok(())
}

#[tokio::test]
async fn fk_class_fails_on_another_class() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(CONCEPTS).await?;
    cdm.execute(
        "INSERT INTO drug_era (drug_era_id, person_id, drug_concept_id, drug_era_start_date, \
         drug_era_end_date) VALUES (1, 1, 9101, '2020-01-01', '2020-02-01'), \
         (2, 1, 9102, '2020-03-01', '2020-04-01')",
    )
    .await?;
    let result = cdm
        .check(CheckName::FkClass, "drug_era", Some("drug_concept_id"))
        .await?;
    assert_failed(&result, Some(counts(1, 2)));
    Ok(())
}

#[tokio::test]
async fn is_standard_valid_concept_passes_on_a_standard_concept() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(CONCEPTS).await?;
    cdm.execute(PERSONS).await?;
    let result = cdm
        .check(
            CheckName::IsStandardValidConcept,
            "person",
            Some("gender_concept_id"),
        )
        .await?;
    assert_passed(&result, counts(0, 2));
    Ok(())
}

#[tokio::test]
async fn is_standard_valid_concept_fails_on_a_retired_concept() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(CONCEPTS).await?;
    cdm.execute(PERSONS).await?;
    cdm.execute("UPDATE person SET gender_concept_id = 9003 WHERE person_id = 2")
        .await?;
    let result = cdm
        .check(
            CheckName::IsStandardValidConcept,
            "person",
            Some("gender_concept_id"),
        )
        .await?;
    assert_failed(&result, Some(counts(1, 2)));
    Ok(())
}

/// One visit for person 1.
const VISIT: &str = "INSERT INTO visit_occurrence (visit_occurrence_id, person_id, \
    visit_concept_id, visit_start_date, visit_end_date, visit_type_concept_id) \
    VALUES (1, 1, 0, '2026-01-01', '2026-01-02', 0)";

/// Conditions for persons 1 and 2, the second with concept 0.
const CONDITIONS: &str = "INSERT INTO condition_occurrence (condition_occurrence_id, person_id, \
    condition_concept_id, condition_start_date, condition_type_concept_id) \
    VALUES (1, 1, 9002, '2026-01-01', 0), (2, 2, 0, '2026-01-01', 0)";

/// Drug exposures for person 1; `route` is the second row's route concept
/// and source value.
fn drugs(route: &str) -> String {
    format!(
        "INSERT INTO drug_exposure (drug_exposure_id, person_id, drug_concept_id, \
         drug_exposure_start_date, drug_exposure_end_date, drug_type_concept_id, \
         route_concept_id, route_source_value) VALUES \
         (1, 1, 9102, '2026-01-01', '2026-01-02', 0, 9101, 'oral'), \
         (2, 1, 9102, '2026-01-01', '2026-01-02', 0, {route})"
    )
}

#[tokio::test]
async fn person_completeness_passes_when_every_person_has_a_row() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(
        "INSERT INTO person (person_id, gender_concept_id, year_of_birth, \
                 race_concept_id, ethnicity_concept_id) VALUES (1, 9001, 1980, 0, 0)",
    )
    .await?;
    cdm.execute(VISIT).await?;
    let result = cdm
        .check(
            CheckName::MeasurePersonCompleteness,
            "visit_occurrence",
            None,
        )
        .await?;
    assert_passed(&result, counts(0, 1));
    Ok(())
}

#[tokio::test]
async fn person_completeness_fails_above_its_threshold() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    let result = cdm
        .check(
            CheckName::MeasurePersonCompleteness,
            "visit_occurrence",
            None,
        )
        .await?;
    assert_failed(&result, Some(counts(2, 2)));
    Ok(())
}

#[tokio::test]
async fn condition_era_completeness_passes_when_every_condition_has_an_era()
-> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(CONDITIONS).await?;
    cdm.execute(
        "INSERT INTO condition_era (condition_era_id, person_id, condition_concept_id, \
         condition_era_start_date, condition_era_end_date) \
         VALUES (1, 1, 9002, '2026-01-01', '2026-01-01')",
    )
    .await?;
    let result = cdm
        .check(
            CheckName::MeasureConditionEraCompleteness,
            "condition_era",
            None,
        )
        .await?;
    assert_passed(&result, counts(0, 1));
    Ok(())
}

#[tokio::test]
async fn condition_era_completeness_fails_on_a_person_with_no_era() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(CONDITIONS).await?;
    let result = cdm
        .check(
            CheckName::MeasureConditionEraCompleteness,
            "condition_era",
            None,
        )
        .await?;
    assert_failed(&result, Some(counts(1, 1)));
    Ok(())
}

#[tokio::test]
async fn value_completeness_passes_below_its_threshold() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(&drugs("NULL, NULL")).await?;
    let result = cdm
        .check(
            CheckName::MeasureValueCompleteness,
            "drug_exposure",
            Some("route_concept_id"),
        )
        .await?;
    assert_passed(&result, counts(1, 2));
    Ok(())
}

#[tokio::test]
async fn value_completeness_fails_above_its_threshold() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(&drugs("NULL, NULL")).await?;
    let check = planned(
        CheckName::MeasureValueCompleteness,
        "drug_exposure",
        Some("route_source_value"),
    )?;
    cdm.execute("UPDATE drug_exposure SET route_source_value = NULL")
        .await?;
    let result = run::run_check(&cdm.pool, &check).await;
    let threshold = result
        .threshold_value
        .map(omop_cdm::quality::definitions::Percentage::value);
    assert_eq!(Some(counts(2, 2)), result.counts);
    assert_eq!(threshold != Some(100), result.failed, "{threshold:?}");
    Ok(())
}

#[tokio::test]
async fn standard_concept_record_completeness_passes_without_concept_zero()
-> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(&drugs("9101, 'oral'")).await?;
    let result = cdm
        .check(
            CheckName::StandardConceptRecordCompleteness,
            "drug_exposure",
            Some("route_concept_id"),
        )
        .await?;
    assert_passed(&result, counts(0, 2));
    Ok(())
}

#[tokio::test]
async fn standard_concept_record_completeness_counts_zero_and_a_null_beside_a_source_value()
-> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(&drugs("NULL, 'rectal'")).await?;
    cdm.execute("UPDATE drug_exposure SET route_concept_id = 0 WHERE drug_exposure_id = 1")
        .await?;
    let check = planned(
        CheckName::StandardConceptRecordCompleteness,
        "drug_exposure",
        Some("route_concept_id"),
    )?;
    let result = run::run_check(&cdm.pool, &check).await;
    assert_eq!(
        Some(counts(2, 2)),
        result.counts,
        "concept 0 and a NULL beside 'rectal' both violate"
    );
    Ok(())
}

#[tokio::test]
async fn source_concept_record_completeness_fails_on_concept_zero() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    cdm.execute(
        "INSERT INTO condition_occurrence (condition_occurrence_id, person_id, \
         condition_concept_id, condition_start_date, condition_type_concept_id, \
         condition_source_concept_id) VALUES (1, 1, 9002, '2026-01-01', 0, 0)",
    )
    .await?;
    let check = find(
        &Plan::new(&PlanOptions::default())?,
        CheckName::SourceConceptRecordCompleteness,
        "condition_occurrence",
        Some("condition_source_concept_id"),
    )
    .ok_or("the plan checks condition_source_concept_id")?;
    let result = run::run_check(&cdm.pool, &check).await;
    assert_eq!(Some(counts(1, 1)), result.counts);
    cdm.execute("UPDATE condition_occurrence SET condition_source_concept_id = 9002")
        .await?;
    let result = run::run_check(&cdm.pool, &check).await;
    assert_eq!(Some(counts(0, 1)), result.counts);
    Ok(())
}

#[tokio::test]
async fn source_value_completeness_counts_distinct_unmapped_values() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(
        "INSERT INTO person (person_id, gender_concept_id, year_of_birth, race_concept_id, \
         ethnicity_concept_id, gender_source_value) VALUES \
         (1, 9001, 1980, 0, 0, 'F'), (2, 9001, 1990, 0, 0, 'F'), \
         (3, 0, 1990, 0, 0, 'X'), (4, 0, 1990, 0, 0, NULL)",
    )
    .await?;
    let result = cdm
        .check(
            CheckName::SourceValueCompleteness,
            "person",
            Some("gender_source_value"),
        )
        .await?;
    assert_eq!(
        Some(counts(2, 3)),
        result.counts,
        "'X' and NULL are unmapped, of 'F', 'X' and NULL"
    );
    cdm.execute("UPDATE person SET gender_concept_id = 9001")
        .await?;
    let result = cdm
        .check(
            CheckName::SourceValueCompleteness,
            "person",
            Some("gender_source_value"),
        )
        .await?;
    assert_passed(&result, counts(0, 3));
    Ok(())
}

#[tokio::test]
async fn a_run_marks_the_checks_of_an_empty_table_not_applicable() -> Result<(), Box<dyn Error>> {
    let Some(cdm) = cdm().await? else {
        return Ok(());
    };
    cdm.execute(PERSONS).await?;
    let plan = Plan::new(&PlanOptions::default())?;
    let results = run::run(&cdm.pool, &plan).await;
    let route = results
        .iter()
        .find(|result| {
            result.check_name == CheckName::StandardConceptRecordCompleteness
                && result.cdm_field_name.as_deref() == Some("ROUTE_CONCEPT_ID")
        })
        .ok_or("the route check ran")?;
    assert!(route.not_applicable, "{route:?}");
    assert_eq!(
        Some("Table DRUG_EXPOSURE is empty."),
        route.not_applicable_reason.as_deref()
    );
    let year = results
        .iter()
        .find(|result| {
            result.check_name == CheckName::MeasureValueCompleteness
                && result.cdm_table_name == "PERSON"
                && result.cdm_field_name.as_deref() == Some("YEAR_OF_BIRTH")
        })
        .ok_or("the year of birth check ran")?;
    assert!(year.passed && !year.not_applicable, "{year:?}");
    Ok(())
}
