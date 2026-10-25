// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OMOP round trip, the v0.0.4 acceptance test (`docs/architecture.md`
//! §11), behind the `FERROBRIDGE_E2E` gate.
//!
//! The reference CDR holds three synthetic laboratory compositions in two
//! EHRs, committed over ITS-REST 1.1.0 as canonical JSON. `etl run` reads
//! them over `POST /query/aql` through the job the binary calls, maps them
//! with the published OMOCL laboratory files copied verbatim from
//! `docs/specs/omocl`, and writes them into a CDM v5.4 PostgreSQL built from
//! the vendored DDL with the synthetic vocabulary loaded. The rows are a
//! reviewed snapshot over a projection that drops the surrogate ids and the
//! run's own identifiers.

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::cdr::ids::versioned_object_uid;
use ferrobridge_server::etl::report::{Persons, RunReport};
use ferrobridge_testkit::containers::{self, Postgres};
use ferrobridge_testkit::fixtures::LABORATORY_REPORT_OPT;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;
use std::collections::BTreeMap;
use std::error::Error;

use super::support::{
    LABORATORIES, build_cdm, canonical, configuration, copy_mappings, create, create_ehr, etl_run,
    rows, update, upload_template,
};
/// The deterministic projection of the `MEASUREMENT` rows: the composition,
/// the natural key, the person's EHR and the visit's source in place of the
/// surrogate ids.
const MEASUREMENTS: &str = "SELECT r.versioned_object_uid, r.archetype_root_path, r.occurrence_path, r.mapping, \
    r.entry, r.branch, p.ehr_id, v.visit_source_value, m.measurement_concept_id, \
    m.measurement_source_value, m.measurement_source_concept_id, m.measurement_date, \
    m.measurement_datetime, m.value_as_number, m.value_source_value, m.unit_concept_id, \
    m.unit_source_value, m.unit_source_concept_id, m.range_low, m.range_high, \
    m.operator_concept_id, m.measurement_type_concept_id \
    FROM ferrobridge.record r \
    JOIN cdm.measurement m ON r.cdm_table = 'measurement' AND m.measurement_id = r.surrogate_id \
    JOIN ferrobridge.person_map p ON p.person_id = m.person_id \
    LEFT JOIN cdm.visit_occurrence v ON v.visit_occurrence_id = m.visit_occurrence_id";

/// The projection of the `SPECIMEN` rows.
const SPECIMENS: &str = "SELECT r.versioned_object_uid, r.archetype_root_path, r.occurrence_path, p.ehr_id, \
    s.specimen_concept_id, s.specimen_source_value, s.specimen_date, s.specimen_datetime, \
    s.specimen_type_concept_id \
    FROM ferrobridge.record r \
    JOIN cdm.specimen s ON r.cdm_table = 'specimen' AND s.specimen_id = r.surrogate_id \
    JOIN ferrobridge.person_map p ON p.person_id = s.person_id";

/// The projection of the `FACT_RELATIONSHIP` rows: each end named by its
/// table and natural key.
const FACTS: &str = "SELECT f.domain_concept_id_1, r1.cdm_table, r1.versioned_object_uid, \
    r1.archetype_root_path, r1.occurrence_path, f.domain_concept_id_2, r2.cdm_table, \
    r2.archetype_root_path, r2.occurrence_path, \
    f.relationship_concept_id \
    FROM cdm.fact_relationship f \
    JOIN ferrobridge.record r1 ON r1.surrogate_id = f.fact_id_1 \
      AND r1.cdm_table = CASE f.domain_concept_id_1 WHEN 9011 THEN 'measurement' ELSE 'specimen' END \
    JOIN ferrobridge.record r2 ON r2.surrogate_id = f.fact_id_2 \
      AND r2.cdm_table = CASE f.domain_concept_id_2 WHEN 9011 THEN 'measurement' ELSE 'specimen' END";

/// The projection of the `VISIT_OCCURRENCE` and `OBSERVATION_PERIOD` rows.
const VISITS_AND_PERIODS: &str = "SELECT 'visit', p.ehr_id, v.visit_source_value, \
    v.visit_concept_id, v.visit_start_date, v.visit_start_datetime, v.visit_end_date, \
    v.visit_end_datetime, v.visit_type_concept_id \
    FROM cdm.visit_occurrence v JOIN ferrobridge.person_map p ON p.person_id = v.person_id \
    UNION ALL SELECT 'observation_period', p.ehr_id, NULL, NULL, \
    o.observation_period_start_date, NULL, o.observation_period_end_date, NULL, \
    o.period_type_concept_id \
    FROM cdm.observation_period o JOIN ferrobridge.person_map p ON p.person_id = o.person_id";

/// The projection of the `PERSON` rows: the EHR in place of the surrogate id.
const PERSONS: &str = "SELECT m.ehr_id, p.gender_concept_id, p.year_of_birth, \
    p.race_concept_id, p.ethnicity_concept_id, p.person_source_value \
    FROM cdm.person p JOIN ferrobridge.person_map m ON m.person_id = p.person_id";

/// Asserts that each of the two EHRs has its placeholder `PERSON` row, since
/// the laboratory files map no `PERSON`, and that no row names a missing
/// person.
async fn assert_placeholders(
    postgres: &Postgres,
    report: &RunReport,
) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        Persons {
            total: 2,
            placeholders: 2,
        },
        report.persons,
        "{report}"
    );
    assert_eq!(
        0,
        crate::etl::orphans(postgres).await?,
        "every person_id names a PERSON"
    );
    Ok(())
}

/// Renders the rows of `query`, identifiers replaced by the names in
/// `names`, sorted.
async fn projected(
    postgres: &Postgres,
    query: &str,
    names: &BTreeMap<String, String>,
) -> Result<String, Box<dyn Error>> {
    let mut lines: Vec<String> = rows(postgres, query)
        .await?
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| names.get(&cell).cloned().unwrap_or(cell))
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .collect();
    lines.sort();
    Ok(lines.join("\n"))
}

/// Renders the whole projection the snapshot pins.
async fn projection(
    postgres: &Postgres,
    names: &BTreeMap<String, String>,
) -> Result<String, Box<dyn Error>> {
    Ok(format!(
        "PERSON\n{}\n\nMEASUREMENT\n{}\n\nSPECIMEN\n{}\n\nFACT_RELATIONSHIP\n{}\n\nVISIT_OCCURRENCE and OBSERVATION_PERIOD\n{}\n",
        projected(postgres, PERSONS, names).await?,
        projected(postgres, MEASUREMENTS, names).await?,
        projected(postgres, SPECIMENS, names).await?,
        projected(postgres, FACTS, names).await?,
        projected(postgres, VISITS_AND_PERIODS, names).await?,
    ))
}

/// Returns the data of the CDM and bridge schemas as `pg_dump` writes it,
/// each table's rows sorted and the watermark's run columns cut.
async fn dump(postgres: &Postgres) -> Result<String, Box<dyn Error>> {
    crate::etl::dump(postgres).await
}

/// Runs the ported Data Quality Dashboard plan over the round trip's CDM and
/// asserts that every conformance check passes and that the concept record
/// completeness checks count the concept 0 rows the run report counted,
/// column by column.
///
/// A column whose template also counts a NULL beside a source value is
/// compared as at least the report's count.
async fn assert_quality_agrees(
    postgres: &Postgres,
    report: &RunReport,
) -> Result<(), Box<dyn Error>> {
    use omop_cdm::quality::check::{CheckName, CheckParameters};
    use omop_cdm::quality::plan::{Plan, PlanOptions};

    let pool = omop_cdm::database::CdmPool::connect(
        sqlx::postgres::PgPoolOptions::new().max_connections(1),
        postgres.url().parse()?,
        omop_cdm::ddl::SchemaName::new("cdm")?,
    )
    .await?;
    let plan = Plan::new(&PlanOptions::default())?;
    let results = omop_cdm::quality::run::run(&pool, &plan).await;
    for result in &results {
        assert!(
            !result.is_error,
            "{} erred: {:?}",
            result.check_id, result.error
        );
    }
    let failing: Vec<&str> = results
        .iter()
        .filter(|result| {
            result.category == omop_cdm::quality::definitions::KahnCategory::Conformance
                && result.failed
        })
        .map(|result| result.check_id.as_str())
        .collect();
    assert!(failing.is_empty(), "conformance checks failed: {failing:?}");
    let mut reported: BTreeMap<String, u64> = BTreeMap::new();
    for totals in report.mappings.values() {
        for (column, count) in &totals.concept_zero {
            *reported.entry(column.to_uppercase()).or_default() += count;
        }
    }
    assert!(!reported.is_empty(), "the round trip counts a concept 0");
    for (column, count) in &reported {
        let (table, field) = column.split_once('.').ok_or("a table.column key")?;
        let check = plan
            .checks()
            .iter()
            .find(|check| {
                check.name() == CheckName::StandardConceptRecordCompleteness
                    && check.cdm_table_name() == table
                    && check.cdm_field_name().as_deref() == Some(field)
            })
            .ok_or_else(|| format!("no standardConceptRecordCompleteness on {column}"))?;
        let result = results
            .iter()
            .find(|result| result.check_id == check.id())
            .ok_or("the check ran")?;
        let violated = result.counts.ok_or("counts")?.violated;
        if check.parameters() == CheckParameters::SourceValue(None) {
            assert_eq!(*count, violated, "{column}: the check and the report agree");
        } else {
            assert!(violated >= *count, "{column}: {violated} below {count}");
        }
    }
    pool.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn the_laboratory_round_trip_writes_the_reviewed_rows_and_a_rerun_changes_nothing()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let cdr = containers::cdr().await?;
    let postgres = containers::postgres().await?;
    build_cdm(&postgres).await?;
    upload_template(&cdr).await?;
    let client = CdrClient::new(&CdrConfig::new(format!("{}/", cdr.base_url()).parse()?))?;
    let ehrs = [create_ehr(&client).await?, create_ehr(&client).await?];

    let index = WebTemplateIndex::build(&TemplateSource::opt14(LABORATORY_REPORT_OPT)?)?;
    let mut names = BTreeMap::new();
    for (position, ehr) in ehrs.iter().enumerate() {
        names.insert(ehr.as_str().to_owned(), format!("ehr-{}", position + 1));
    }
    let mut versions = Vec::new();
    for laboratory in &LABORATORIES {
        let ehr = ehrs.get(laboratory.ehr).ok_or("an EHR")?;
        let version = create(&client, ehr, &canonical(&index, laboratory)?).await?;
        names.insert(
            versioned_object_uid(&version).value().to_owned(),
            laboratory.label.to_owned(),
        );
        versions.push(version);
    }

    let scratch = tempfile::tempdir()?;
    let mappings = tempfile::tempdir()?;
    copy_mappings(mappings.path())?;
    let config = configuration(scratch.path(), &cdr, &postgres, mappings.path())?;

    let first = etl_run(&config).await?;
    assert_eq!(3, first.compositions.committed, "{first}");
    assert!(first.refusals.is_empty(), "{first}");
    assert_eq!(3, first.compositions.with_visit, "{first}");
    assert_eq!(Some(&6), first.rows.get("measurement"), "{first}");
    assert_eq!(Some(&1), first.rows.get("specimen"), "{first}");
    assert_eq!(2, first.derived.visit_occurrence, "{first}");
    assert_eq!(2, first.derived.observation_period, "{first}");
    assert_placeholders(&postgres, &first).await?;
    let zero = first
        .mappings
        .get("Laboratory_test_analyte_v1")
        .and_then(|totals| {
            totals
                .concept_zero
                .get("measurement.measurement_concept_id")
        });
    assert_eq!(Some(&1), zero, "the unmapped analyte is counted: {first}");
    assert_quality_agrees(&postgres, &first).await?;
    assert_eq!(
        vec![vec![String::from("0")]],
        rows(
            &postgres,
            "SELECT count(*) FROM cdm.measurement WHERE visit_occurrence_id IS NULL"
        )
        .await?,
        "every clinical row carries its visit"
    );
    insta::assert_snapshot!(
        "laboratory_round_trip",
        projection(&postgres, &names).await?
    );
    let before = dump(&postgres).await?;

    let second = etl_run(&config).await?;
    assert_eq!(3, second.compositions.committed, "{second}");
    assert_eq!(
        before,
        dump(&postgres).await?,
        "a second run leaves the data as it was"
    );

    let altered = LABORATORIES.get(1).ok_or("the second composition")?;
    let mut changed = *altered;
    if let Some(analyte) = changed.analytes.first_mut() {
        analyte.magnitude = 5.4;
    }
    let preceding = versions.get(1).ok_or("the second version")?;
    let ehr = ehrs.get(altered.ehr).ok_or("an EHR")?;
    update(&client, ehr, preceding, &canonical(&index, &changed)?).await?;

    let rows_before = projection(&postgres, &names).await?;
    let third = etl_run(&config).await?;
    assert_eq!(3, third.compositions.committed, "{third}");
    assert_ne!(
        before,
        dump(&postgres).await?,
        "the altered composition changes the data"
    );
    let rows_after = projection(&postgres, &names).await?;
    let differing: Vec<(&str, &str)> = rows_before
        .lines()
        .zip(rows_after.lines())
        .filter(|(was, is)| was != is)
        .collect();
    assert_eq!(1, differing.len(), "exactly one row changed: {differing:?}");
    let (was, is) = differing.first().ok_or("the changed row")?;
    assert!(
        was.starts_with("lab-2 |") && was.contains("| 5.1 |") && is.contains("| 5.4 |"),
        "the altered analyte of lab-2 is the row replaced: {was} / {is}"
    );
    Ok(())
}
