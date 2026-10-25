// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `etl fhir`: FHIR R4 resources into the CDM through an in-memory
//! composition, with no CDR (`docs/architecture.md` §5.6, our own design).
//!
//! The FHIRconnect set is the operations lane's synthetic Condition context
//! over the testkit's diagnosis template, and the OMOCL set one
//! FerroBRIDGE-authored `ConditionOccurrence` mapping over the problem name,
//! dated by the composition's context. Every resource is synthetic. The
//! container-backed cases run only when `FERROBRIDGE_E2E=1`.

use std::error::Error;

use ferrobridge_server::config::MappingSettings;
use ferrobridge_server::etl::EtlSettings;
use ferrobridge_server::etl::deletions::DeletionPolicy;
use ferrobridge_server::etl::fhir::{self, FhirSide};
use ferrobridge_server::etl::mapper::{CdmVocabulary, OmoclMapper, read_set};
use ferrobridge_server::mappings;
use ferrobridge_testkit::containers;
use omop_cdm::database::{self, CdmPool};
use omop_cdm::ddl::SchemaName;
use omop_cdm::vocabulary::ConceptResolver;
use omop_cdm::writer::CdmWriter;
use omop_cdm::writer::input::{PersonPolicy, RunId};
use serde_json::{Value, json};

use crate::operations::PROFILE;
use crate::operations::mapping_tree;

/// A FerroBRIDGE-authored `ConditionOccurrence` over the problem name, dated
/// by the composition's context.
const CONDITION_OMOCL: &str = r#"grammar: OMOCL/v1.0.0
type: model
metadata:
  name: Synthetic_problem_diagnosis_v1
  version: 1.0.0
spec:
  system: OMOP
  version: 5.4
  openEhrConfig:
    archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1

mappings:
  - type: "ConditionOccurrence"
    concept_id:
      alternatives:
        - path: "data[at0001]/items[at0002]"
    condition_start_date:
      alternatives:
        - path: "../context"
"#;

/// The time every synthetic resource was last updated.
const UPDATED: &str = "2026-10-01T09:00:00Z";

/// A synthetic Condition of `patient` claiming the context's profile.
fn condition(id: &str, version: &str) -> Value {
    json!({
        "resourceType": "Condition",
        "id": id,
        "meta": {"profile": [PROFILE], "versionId": version, "lastUpdated": UPDATED},
        "code": {"coding": [{"system": "http://snomed.info/sct", "code": "SYN-COND-1",
                             "display": "Synthetic condition"}],
                 "text": "Synthetic condition"},
        "subject": {"reference": "Patient/synthetic-subject-0001"}
    })
}

/// The settings of the run.
fn settings() -> Result<EtlSettings, Box<dyn Error>> {
    Ok(EtlSettings {
        compositions: None,
        page_size: ferrobridge_server::cdr::query::PageSize::new(100)?,
        type_concept_id: 32817,
        observation_period_type_concept_id: 32880,
        visits: None,
        deletions: DeletionPolicy::Remove,
        aliases: omocl::engine::concept::VocabularyAliases::default(),
        quality_checks: false,
        derived_tables: omop_cdm::derived::Scope::OwnedPersons,
    })
}

/// Counts the rows of `table` in the CDM schema.
async fn count(pool: &CdmPool, table: &str) -> Result<i64, Box<dyn Error>> {
    Ok(
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
            .fetch_one(pool.pool())
            .await?,
    )
}

/// Builds the CDM and the bridge schema on `postgres`.
async fn cdm(postgres: &containers::Postgres) -> Result<(CdmPool, CdmWriter), Box<dyn Error>> {
    let pool = CdmPool::connect(
        sqlx::postgres::PgPoolOptions::new().max_connections(2),
        postgres.url().parse()?,
        SchemaName::new("cdm")?,
    )
    .await?;
    database::init(&pool).await?;
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

#[tokio::test]
async fn a_condition_lands_in_the_cdm_with_no_cdr_and_a_rerun_changes_nothing()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let (pool, mut writer) = cdm(&postgres).await?;

    let tree = mapping_tree()?;
    let programs = mappings::load(&MappingSettings {
        directory: tree.path().join("mappings"),
        templates: tree.path().join("templates"),
    })?;
    let omocl = tempfile::tempdir()?;
    std::fs::write(
        omocl.path().join("Synthetic_problem_diagnosis_v1.yml"),
        CONDITION_OMOCL,
    )?;
    let mapper = OmoclMapper::new(
        read_set(omocl.path())?,
        CdmVocabulary::new(ConceptResolver::new(pool.clone())),
        omocl::engine::concept::VocabularyAliases::default(),
    );
    let side = FhirSide {
        programs: &programs,
        device: "Device/ferrobridge-test",
        composer: "FHIRconnect",
        language: Some("en"),
        territory: Some("NL"),
        now: UPDATED,
    };
    let resources = vec![
        condition("c-1", "1"),
        json!({"resourceType": "Observation", "id": "o-1", "status": "final",
               "code": {"text": "Synthetic observation"},
               "subject": {"reference": "Patient/synthetic-subject-0001"}}),
    ];
    let first = fhir::run(
        &resources,
        &side,
        &settings()?,
        &mut writer,
        &mapper,
        &RunId::new("run-1")?,
    )
    .await?;
    let counted = first.resources.ok_or("the run counts its resources")?;
    assert_eq!(
        (2, 1, 1, 0),
        (
            counted.read,
            counted.mapped,
            counted.unmapped,
            counted.refused
        ),
        "{first}"
    );
    assert_eq!(1, first.compositions.committed, "{first}");
    assert_eq!(1, count(&pool, "condition_occurrence").await?);
    assert_eq!(
        1,
        count(&pool, "person").await?,
        "the subject became a person"
    );

    let second = fhir::run(
        &resources,
        &side,
        &settings()?,
        &mut writer,
        &mapper,
        &RunId::new("run-2")?,
    )
    .await?;
    assert_eq!(1, second.compositions.committed, "{second}");
    assert_eq!(
        1,
        count(&pool, "condition_occurrence").await?,
        "nothing is appended"
    );
    let ids: Vec<i32> =
        sqlx::query_scalar("SELECT condition_occurrence_id FROM condition_occurrence")
            .fetch_all(pool.pool())
            .await?;
    assert_eq!(vec![1], ids, "the row keeps its id");

    let third = fhir::run(
        &[condition("c-1", "2")],
        &side,
        &settings()?,
        &mut writer,
        &mapper,
        &RunId::new("run-3")?,
    )
    .await?;
    assert_eq!(1, third.compositions.committed, "{third}");
    let version: String = sqlx::query_scalar("SELECT version_uid FROM ferrobridge.watermark")
        .fetch_one(pool.pool())
        .await?;
    assert!(version.ends_with("::ferrobridge.fhir::2"), "{version}");
    assert_eq!(1, count(&pool, "condition_occurrence").await?);
    Ok(())
}

#[test]
fn an_ndjson_file_and_a_bundle_read_their_resources_in_order() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let ndjson = dir.path().join("Condition.ndjson");
    std::fs::write(
        &ndjson,
        format!("{}\n\n{}\n", condition("c-1", "1"), condition("c-2", "1")),
    )?;
    let ids: Vec<Value> = fhir::read_input(&ndjson)?
        .iter()
        .map(|resource| resource["id"].clone())
        .collect();
    assert_eq!(vec![json!("c-1"), json!("c-2")], ids);
    let bundle = dir.path().join("bundle.json");
    std::fs::write(
        &bundle,
        json!({"resourceType": "Bundle", "type": "collection",
               "entry": [{"resource": condition("c-3", "1")}]})
        .to_string(),
    )?;
    assert_eq!(1, fhir::read_input(&bundle)?.len());
    let broken = dir.path().join("broken.ndjson");
    std::fs::write(&broken, "{\"resourceType\": \"Condition\"}\n{not json}\n")?;
    let refused = fhir::read_input(&broken).expect_err("line 2 is no JSON");
    assert!(
        matches!(refused, fhir::InputError::Json { line: 2, .. }),
        "{refused:?}"
    );
    let untyped = dir.path().join("untyped.json");
    std::fs::write(&untyped, "{\"id\": \"x\"}")?;
    assert!(matches!(
        fhir::read_input(&untyped),
        Err(fhir::InputError::NotAResource { item: 1, .. })
    ));
    Ok(())
}

/// The profile the synthetic procedure context claims.
const PROCEDURE_PROFILE: &str =
    "http://example.org/fhir/StructureDefinition/ferrobridge-etl-procedure";

/// A synthetic context mapping a `Procedure` into the published
/// `KDS_Prozedur` template.
const PROCEDURE_CONTEXT: &str = r#"grammar: FHIRConnect/v1.0.0
type: context
metadata:
  name: ferrobridge_etl_procedure.context
  version: 1.0.0
spec:
  system: FHIR
  version: R4
context:
  profile:
    url: "http://example.org/fhir/StructureDefinition/ferrobridge-etl-procedure"
  template:
    id: "KDS_Prozedur"
  archetypes:
    - "ferrobridge_etl_procedure"
  start: "ferrobridge_etl_procedure"
"#;

/// A synthetic model mapping: the procedure's name and its completed state.
const PROCEDURE_MODEL: &str = r#"grammar: FHIRConnect/v1.0.0
type: model
metadata:
  name: ferrobridge_etl_procedure
  version: 1.0.0
spec:
  system: FHIR
  version: R4
  openEhrConfig:
    archetype: openEHR-EHR-ACTION.procedure.v1
  fhirConfig:
    structureDefinition: http://hl7.org/fhir/StructureDefinition/Procedure

mappings:
  - name: "procedureName"
    with:
      fhir: "$resource.code"
      openehr: "$archetype/description[at0001]/items[at0002]"
      type: "CODEABLECONCEPT"

  - name: "completed"
    unidirectional: "fhir->openehr"
    with:
      fhir: "$resource"
      openehr: "$archetype/ism_transition/current_state"
      type: NONE
    manual:
      - name: "completed"
        openehr:
          - path: "defining_code/terminology_id/value"
            value: "openehr"
          - path: "defining_code/code_string"
            value: "532"
          - path: "value"
            value: "Completed"
        fhirCondition:
          targetRoot: "$resource"
          targetAttribute: "status"
          operator: "one of"
          criteria: "completed"
"#;

/// The logical id of the synthetic MII Person `Patient` of the testkit.
const PATIENT: &str = "ferrobridge-kds-patient-1";

/// Returns the synthetic MII Person `Patient` of the testkit.
fn patient() -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_str(
        ferrobridge_testkit::fixtures::KDS_PERSON_PATIENT,
    )?)
}

/// A synthetic Procedure of the testkit's `Patient`, performed on 2 October.
fn procedure() -> Value {
    json!({
        "resourceType": "Procedure",
        "id": "p-1",
        "meta": {"profile": [PROCEDURE_PROFILE], "versionId": "1", "lastUpdated": UPDATED},
        "status": "completed",
        "code": {"coding": [{"system": "http://snomed.info/sct", "code": "SYN-PROC-1",
                             "display": "Synthetic procedure"}],
                 "text": "Synthetic procedure"},
        "subject": {"reference": format!("Patient/{PATIENT}")},
        "performedDateTime": "2026-10-02T10:30:00Z"
    })
}

/// Writes the condition, the KDS person and the procedure mapping sets, with
/// their templates, into one tree.
fn mixed_tree() -> Result<tempfile::TempDir, Box<dyn Error>> {
    let tree = mapping_tree()?;
    let mappings = tree.path().join("mappings");
    crate::kds::write_person_mappings(&mappings)?;
    std::fs::write(
        mappings.join("ferrobridge_etl_procedure.context.yml"),
        PROCEDURE_CONTEXT,
    )?;
    std::fs::write(
        mappings.join("ferrobridge_etl_procedure.yml"),
        PROCEDURE_MODEL,
    )?;
    let templates = tree.path().join("templates");
    std::fs::write(
        templates.join("KDS_Person.opt"),
        ferrobridge_testkit::fixtures::KDS_PERSON_OPT,
    )?;
    std::fs::write(
        templates.join("KDS_Prozedur.opt"),
        ferrobridge_testkit::fixtures::KDS_PROZEDUR_OPT,
    )?;
    Ok(tree)
}

/// Returns the template a resource's composition names, refusing any other
/// outcome.
fn template_of(outcome: fhir::Outcome) -> Result<(String, Value), Box<dyn Error>> {
    match outcome {
        fhir::Outcome::Mapped {
            composition,
            template,
            ..
        } => Ok((template.to_string(), composition)),
        other => Err(format!("the resource was not mapped: {other:?}").into()),
    }
}

#[test]
fn a_patient_and_a_procedure_become_compositions_of_their_templates() -> Result<(), Box<dyn Error>>
{
    let tree = mixed_tree()?;
    let programs = mappings::load(&MappingSettings {
        directory: tree.path().join("mappings"),
        templates: tree.path().join("templates"),
    })?;
    let side = FhirSide {
        programs: &programs,
        device: "Device/ferrobridge-test",
        composer: "FHIRconnect",
        language: Some("de"),
        territory: Some("DE"),
        now: UPDATED,
    };
    let (template, _) = template_of(fhir::composition_of(&side, &patient()?))?;
    assert_eq!(
        ferrobridge_testkit::fixtures::KDS_PERSON_TEMPLATE_ID,
        template
    );
    let (template, composition) = template_of(fhir::composition_of(&side, &procedure()))?;
    assert_eq!(
        ferrobridge_testkit::fixtures::KDS_PROZEDUR_TEMPLATE_ID,
        template
    );
    let text = composition.to_string();
    assert!(text.contains("Synthetic procedure"), "{text}");
    assert!(
        text.contains("\"current_state\"") && text.contains("\"532\""),
        "{text}"
    );
    Ok(())
}

#[tokio::test]
async fn a_patient_a_condition_and_a_procedure_land_in_their_cdm_tables()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let (pool, mut writer) = cdm(&postgres).await?;
    let tree = mixed_tree()?;
    let programs = mappings::load(&MappingSettings {
        directory: tree.path().join("mappings"),
        templates: tree.path().join("templates"),
    })?;
    let corpus = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/specs/omocl"
    ));
    let omocl = tempfile::tempdir()?;
    std::fs::write(
        omocl.path().join("Synthetic_problem_diagnosis_v1.yml"),
        CONDITION_OMOCL,
    )?;
    for file in [
        "person_data/person_data_v0.yml",
        "medical_data/action/Procedure_v1.yml",
    ] {
        let source = corpus.join(file);
        let name = source.file_name().ok_or("a corpus file has a name")?;
        std::fs::copy(&source, omocl.path().join(name))?;
    }
    let mapper = OmoclMapper::new(
        read_set(omocl.path())?,
        CdmVocabulary::new(ConceptResolver::new(pool.clone())),
        omocl::engine::concept::VocabularyAliases::default(),
    );
    let side = FhirSide {
        programs: &programs,
        device: "Device/ferrobridge-test",
        composer: "FHIRconnect",
        language: Some("de"),
        territory: Some("DE"),
        now: UPDATED,
    };
    let mut condition = condition("c-1", "1");
    condition["subject"]["reference"] = json!(format!("Patient/{PATIENT}"));
    let report = fhir::run(
        &[patient()?, condition, procedure()],
        &side,
        &settings()?,
        &mut writer,
        &mapper,
        &RunId::new("run-1")?,
    )
    .await?;
    assert_eq!(3, report.compositions.committed, "{report}");
    assert!(report.refusals.is_empty(), "{report}");
    assert_eq!(1, count(&pool, "person").await?, "one patient, one person");
    assert_eq!(1, count(&pool, "condition_occurrence").await?);
    assert_eq!(1, count(&pool, "procedure_occurrence").await?);
    // TODO(#1226): assert procedure_date is 2026-10-02 once ACTION.time is carried.
    Ok(())
}
