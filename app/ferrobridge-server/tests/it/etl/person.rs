// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The run over the OMOCL engine and a `Person` mapping, the corpus's one
//! (`person_data/person_data_v0.yml`) or FerroBRIDGE's KDS copy, against the
//! published `KDS_Person` template and a synthetic composition built from its
//! FLAT fixture.

use ferrobridge_server::config::Config;
use ferrobridge_server::etl::report::RunReport;
use ferrobridge_server::etl::{RunOptions, run};
use ferrobridge_testkit::containers::{self, Postgres};
use ferrobridge_testkit::fixtures::{KDS_PERSON_FLAT, KDS_PERSON_OPT, KDS_PERSON_TEMPLATE_ID};
use omocl::engine::concept::VocabularyAliases;
use omop_cdm::database::{self, CdmPool};
use omop_cdm::ddl::SchemaName;
use omop_cdm::writer::{CdmWriter, input::PersonPolicy, input::RunId};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::COMPOSITIONS_UIDS;
use super::EHR;
use super::client;
use super::count;
use super::settings;

/// The corpus's `Person` mapping.
const PERSON_DATA: &str = "person_data/person_data_v0.yml";

/// FerroBRIDGE's `Person` mapping for `KDS_Person`.
const KDS_PERSON_DATA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/omocl/tests/fixtures/projects/ferrobridge/kds_person/kds_person_data.yml"
);

/// The FLAT path of the administrative gender's terminology.
const GENDER_TERMINOLOGY: &str = "person/geschlecht/administratives_geschlecht|terminology";

/// The terminology id `KDS_Person` binds the administrative gender to, longer
/// than the 20 characters of OMOP CDM v5.4 `VOCABULARY.vocabulary_id`.
const V2_0001: &str = "http://terminology.hl7.org/CodeSystem/v2-0001";

/// The synthetic `Gender` concept the testkit vocabulary carries under
/// `FB-GENDER` `SYN-GENDER-F`.
const GENDER_CONCEPT: i64 = 6001;

/// Returns the FLAT person fixture.
fn flat() -> Result<Map<String, Value>, Box<dyn Error>> {
    Ok(serde_json::from_str(KDS_PERSON_FLAT)?)
}

/// Returns the canonical person composition `flat` describes.
fn person_composition(flat: &Map<String, Value>) -> Result<Value, Box<dyn Error>> {
    let source = openehr_mapping_core::template::TemplateSource::opt14(KDS_PERSON_OPT)?;
    let index = openehr_mapping_core::index::WebTemplateIndex::build(&source)?;
    Ok(index
        .build_from_flat(flat, "2026-10-01T09:00:00Z")?
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
    let download = omop_cdm::athena::Export::read(&ferrobridge_testkit::vocabulary::directory())?;
    omop_cdm::athena::load(&pool, &download).await?;
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
async fn person_cdr(composition: Value) -> Result<MockServer, Box<dyn Error>> {
    let uid = COMPOSITIONS_UIDS.first().ok_or("a composition uid")?;
    let row = json!([EHR, uid, format!("{uid}::ferrobridge.test::1"), composition]);
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

/// Runs the mapping file at `mapping` over the person `flat` describes, with
/// `aliases`, and returns the report.
async fn run_person(
    postgres: &Postgres,
    mapping: &Path,
    flat: &Map<String, Value>,
    aliases: VocabularyAliases,
) -> Result<RunReport, Box<dyn Error>> {
    let (pool, mut writer) = cdm(postgres).await?;
    let mappings = tempfile::tempdir()?;
    let name = mapping.file_name().ok_or("a mapping file name")?;
    std::fs::copy(mapping, mappings.path().join(name))?;
    let mapper = ferrobridge_server::etl::mapper::OmoclMapper::new(
        ferrobridge_server::etl::mapper::read_set(mappings.path())?,
        ferrobridge_server::etl::mapper::CdmVocabulary::new(
            omop_cdm::vocabulary::ConceptResolver::new(pool),
        ),
        aliases.clone(),
    );
    let cdr = person_cdr(person_composition(flat)?).await?;
    let options = RunOptions {
        resume: false,
        since: None,
    };
    let mut etl = settings(false)?;
    etl.aliases = aliases;
    Ok(run(
        &etl,
        &options,
        &client(&cdr)?,
        &mut writer,
        &mapper,
        &RunId::new("run-person")?,
    )
    .await?)
}

/// Returns the vocabulary aliases an `[etl]` section with `table` under
/// `[etl.vocabulary_aliases]` resolves to.
fn configured_aliases(table: &str) -> Result<VocabularyAliases, Box<dyn Error>> {
    let text = format!(
        "[etl]\naql = \"{}\"\ntype_concept_id = 32817\nobservation_period_type_concept_id = 32880\n\
         [etl.vocabulary_aliases]\n{table}\n",
        super::COMPOSITIONS
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    Ok(settings.etl.ok_or("the ETL is configured")?.aliases)
}

#[tokio::test]
async fn the_corpus_person_mapping_writes_the_person_row_with_race_concept_zero()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let corpus = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/specs/omocl");
    let report = run_person(
        &postgres,
        &Path::new(corpus).join(PERSON_DATA),
        &flat()?,
        VocabularyAliases::default(),
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

#[tokio::test]
async fn a_configured_alias_resolves_a_uri_gender_code_through_the_vocabulary()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let mut flat = flat()?;
    flat.insert(GENDER_TERMINOLOGY.to_owned(), Value::from(V2_0001));
    let aliases = configured_aliases(&format!("\"{V2_0001}\" = \"FB-GENDER\""))?;
    let report = run_person(&postgres, Path::new(KDS_PERSON_DATA), &flat, aliases).await?;
    assert_eq!(1, report.compositions.committed, "{report}");
    assert!(report.refusals.is_empty(), "{report}");
    // OMOP CDM v5.4 `person.gender_concept_id` names the `Gender` domain; the
    // alias names the vocabulary, and the concept comes from `CONCEPT`.
    assert_eq!(
        1,
        count(
            &postgres,
            &format!(
                "SELECT count(*) FROM cdm.person WHERE gender_concept_id = {GENDER_CONCEPT} \
                 AND gender_source_value = 'SYN-GENDER-F'"
            )
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn a_uri_gender_code_with_no_alias_writes_concept_zero_and_keeps_its_source()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let mut flat = flat()?;
    flat.insert(GENDER_TERMINOLOGY.to_owned(), Value::from(V2_0001));
    let report = run_person(
        &postgres,
        Path::new(KDS_PERSON_DATA),
        &flat,
        VocabularyAliases::default(),
    )
    .await?;
    assert_eq!(1, report.compositions.committed, "{report}");
    assert!(report.refusals.is_empty(), "{report}");
    // CDM conventions: "If a Standard Concept does not exist or cannot be
    // identified, the Concept with the CONCEPT_ID 0 is used".
    assert_eq!(
        1,
        count(
            &postgres,
            "SELECT count(*) FROM cdm.person WHERE gender_concept_id = 0 \
             AND gender_source_value = 'SYN-GENDER-F'"
        )
        .await?
    );
    Ok(())
}
