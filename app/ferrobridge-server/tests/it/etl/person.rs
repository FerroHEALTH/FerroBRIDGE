// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The run over the OMOCL engine and the corpus's one `Person` mapping,
//! `person_data/person_data_v0.yml`, against the published `KDS_Person`
//! template and a synthetic composition built from its FLAT fixture.

use ferrobridge_server::etl::{RunOptions, run};
use ferrobridge_testkit::containers::{self, Postgres};
use ferrobridge_testkit::fixtures::{KDS_PERSON_FLAT, KDS_PERSON_OPT, KDS_PERSON_TEMPLATE_ID};
use omop_cdm::database::{self, CdmPool};
use omop_cdm::ddl::SchemaName;
use omop_cdm::writer::{CdmWriter, input::PersonPolicy, input::RunId};
use serde_json::json;
use std::error::Error;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::COMPOSITIONS_UIDS;
use super::EHR;
use super::client;
use super::count;
use super::settings;

/// The corpus's `Person` mapping.
const PERSON_DATA: &str = "person_data/person_data_v0.yml";

/// Returns the canonical person composition, built from its FLAT fixture.
fn person_composition() -> Result<serde_json::Value, Box<dyn Error>> {
    let source = openehr_mapping_core::template::TemplateSource::opt14(KDS_PERSON_OPT)?;
    let index = openehr_mapping_core::index::WebTemplateIndex::build(&source)?;
    let flat: serde_json::Map<String, serde_json::Value> = serde_json::from_str(KDS_PERSON_FLAT)?;
    Ok(index
        .build_from_flat(&flat, "2026-10-01T09:00:00Z")?
        .into_value())
}

/// Starts PostgreSQL with the CDM and the synthetic vocabulary, and returns
/// the writer over it.
async fn cdm(postgres: &Postgres) -> Result<(CdmPool, CdmWriter), Box<dyn Error>> {
    let pool = CdmPool::connect(
        sqlx::postgres::PgPoolOptions::new().max_connections(2),
        postgres.url().parse()?,
        SchemaName::new("cdm")?,
    )
    .await?;
    database::init(&pool).await?;
    let mut connection = pool.pool().acquire().await?;
    ferrobridge_testkit::vocabulary::load(&mut connection).await?;
    drop(connection);
    let mut writer = CdmWriter::connect(
        postgres.url(),
        SchemaName::new("cdm")?,
        SchemaName::new("ferrobridge")?,
        PersonPolicy::CreateOnFirstSight,
    )
    .await?;
    writer.init().await?;
    Ok((pool, writer))
}

/// Returns a stubbed CDR serving one person composition and its template.
async fn person_cdr() -> Result<MockServer, Box<dyn Error>> {
    let uid = COMPOSITIONS_UIDS.first().ok_or("a composition uid")?;
    let row = json!([
        EHR,
        uid,
        format!("{uid}::ferrobridge.test::1"),
        person_composition()?
    ]);
    let cdr = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/query/aql"))
        .and(body_string_contains("AS composition"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "rows": [row] })))
        .mount(&cdr)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/definition/template/adl1.4/{KDS_PERSON_TEMPLATE_ID}"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/xml")
                .set_body_string(KDS_PERSON_OPT),
        )
        .expect(1..)
        .mount(&cdr)
        .await;
    Ok(cdr)
}

#[tokio::test]
async fn the_corpus_person_mapping_writes_the_person_row_with_race_concept_zero()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let (pool, mut writer) = cdm(&postgres).await?;
    let mappings = tempfile::tempdir()?;
    let corpus = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/specs/omocl");
    std::fs::copy(
        std::path::Path::new(corpus).join(PERSON_DATA),
        mappings.path().join("person_data_v0.yml"),
    )?;
    let mapper = ferrobridge_server::etl::mapper::OmoclMapper::new(
        ferrobridge_server::etl::mapper::read_set(mappings.path())?,
        ferrobridge_server::etl::mapper::CdmVocabulary::new(
            omop_cdm::vocabulary::ConceptResolver::new(pool),
        ),
        omocl::engine::concept::VocabularyAliases::default(),
    );
    let cdr = person_cdr().await?;
    let options = RunOptions {
        resume: false,
        since: None,
    };
    let report = run(
        &settings(false)?,
        &options,
        &client(&cdr)?,
        &mut writer,
        &mapper,
        &RunId::new("run-person")?,
    )
    .await?;
    assert_eq!(1, report.compositions.committed, "{report}");
    assert!(report.refusals.is_empty(), "{report}");
    assert_eq!(Some(&1), report.rows.get("person"), "{report}");
    let totals = report
        .mappings
        .get("person_data.v0")
        .ok_or("the person mapping's totals")?;
    assert_eq!(
        Some(&1),
        totals.concept_zero.get("person.race_concept_id"),
        "{report}"
    );
    // `KDS_Person` carries no gender node at `at0019` and no ethnicity node,
    // so the corpus file's own `code: 0` alternatives write both.
    assert_eq!(
        1,
        count(
            &postgres,
            "SELECT count(*) FROM cdm.person WHERE year_of_birth = 1984 \
             AND gender_concept_id = 0 AND race_concept_id = 0 \
             AND ethnicity_concept_id = 0 AND race_source_value IS NULL"
        )
        .await?
    );
    assert_eq!(
        1,
        count(&postgres, "SELECT count(*) FROM cdm.person").await?
    );
    Ok(())
}
