// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `PERSON` from the corpus's `person_data_v0.yml` over the published
//! `KDS_Person` template and a synthetic composition built against it.
//!
//! Every value of the composition and every concept id is invented for the
//! tests.

use std::error::Error;

use omocl::engine::RefusalKind;
use omocl::engine::concept::VocabularyAliases;
use omocl::engine::run;
use omop_cdm::graph::RecordGraph;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;
use serde_json::Value;

use crate::engine::int;
use crate::engine::rows;
use crate::engine::text;
use crate::lab::StubSource;
use crate::lab::compiled;
use crate::lab::concept;
use crate::lab::seams;
use crate::lab::set;
use crate::lab::source;
use crate::lab::standard;
use crate::resolve::mapping;

/// The corpus's one `Person` mapping.
const PERSON_DATA: &str = "person_data/person_data_v0.yml";

/// The person data archetype.
const PERSON_DATA_ARCHETYPE: &str = "openEHR-EHR-ADMIN_ENTRY.person_data.v0";

/// The administrative gender code of the FLAT fixture.
const GENDER_CODE: &str = "SYN-GENDER-F";
/// The synthetic concept the stub resolves [`GENDER_CODE`] to.
const GENDER_CONCEPT: i32 = 8_001;

/// Builds the index over the `KDS_Person` template.
fn template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let source = TemplateSource::opt14(ferrobridge_testkit::fixtures::KDS_PERSON_OPT)?;
    Ok(WebTemplateIndex::build(&source)?)
}

/// Returns the synthetic FLAT person: a birth date and an administrative
/// gender.
fn flat() -> Result<serde_json::Map<String, Value>, Box<dyn Error>> {
    Ok(serde_json::from_str(
        ferrobridge_testkit::fixtures::KDS_PERSON_FLAT,
    )?)
}

/// Runs `corpus` and `inline` over the synthetic person.
async fn person(
    corpus: &[&str],
    inline: &[(&str, &str)],
) -> Result<(RecordGraph, Vec<omocl::engine::RecordRefusal>), Box<dyn Error>> {
    person_from(&flat()?, corpus, inline).await
}

/// Runs `corpus` and `inline` over the person `flat` describes.
async fn person_from(
    flat: &serde_json::Map<String, Value>,
    corpus: &[&str],
    inline: &[(&str, &str)],
) -> Result<(RecordGraph, Vec<omocl::engine::RecordRefusal>), Box<dyn Error>> {
    let index = template()?;
    let composition = index.build_from_flat(flat, "2026-10-01T09:00:00Z")?;
    let program = compiled(&set(corpus, inline)?, &index)?;
    let mut stub = StubSource::default();
    stub.code(
        "FB-GENDER",
        GENDER_CODE,
        standard(concept(
            GENDER_CONCEPT,
            "Gender",
            "FB-GENDER",
            GENDER_CODE,
            true,
        )?),
    );
    let aliases = VocabularyAliases::default();
    let outcome = run(
        &program,
        &composition,
        &index,
        &source()?,
        None,
        &seams(&stub, &aliases),
    )
    .await?;
    Ok(outcome.into_parts())
}

#[tokio::test]
async fn the_corpus_person_mapping_writes_a_person_row() -> Result<(), Box<dyn Error>> {
    let (graph, refusals) = person(&[PERSON_DATA], &[]).await?;
    assert!(refusals.is_empty(), "{refusals:?}");
    let persons = rows(&graph, "person");
    let [row] = persons.as_slice() else {
        panic!("one PERSON row, got {}", persons.len());
    };
    assert_eq!(int(row, "year_of_birth"), Some(1984));
    // `KDS_Person` carries no `at0019` gender node, so the corpus's own
    // `code: 0` alternative is chosen; nor any ethnicity node.
    assert_eq!(int(row, "gender_concept_id"), Some(0));
    assert_eq!(int(row, "ethnicity_concept_id"), Some(0));
    // CDM v5.4 `person.race_concept_id`: "If a clear race or ethnic background
    // cannot be established, use Concept_Id 0"; OMOCL names no race key.
    assert_eq!(int(row, "race_concept_id"), Some(0));
    assert_eq!(row.cell("race_source_value"), None);
    assert_eq!(row.cell("race_source_concept_id"), None);
    Ok(())
}

/// A `Person` mapping over `KDS_Person`'s administrative gender node.
fn gendered(ethnicity: bool) -> String {
    let ethnicity = if ethnicity {
        "    ethnicity_concept_id:\n      alternatives:\n        - code: 0\n"
    } else {
        ""
    };
    mapping(
        "synthetic_person",
        PERSON_DATA_ARCHETYPE,
        &format!(
            "  - type: \"Person\"\n    gender_concept:\n      alternatives:\n        - path: \
             \"../content[openEHR-EHR-EVALUATION.gender.v1]/data[at0002]/items[at0022]\"\n    \
             year_of_birth:\n      alternatives:\n        - path: \
             \"/data[at0001]/items[openEHR-EHR-CLUSTER.person_birth_data_iso.v0]/items[at0001]\"\n\
             {ethnicity}"
        ),
    )
}

#[tokio::test]
async fn a_mapped_gender_is_written_beside_race_concept_zero() -> Result<(), Box<dyn Error>> {
    let source = gendered(true);
    let (graph, refusals) = person(&[], &[("synthetic_person.yml", &source)]).await?;
    assert!(refusals.is_empty(), "{refusals:?}");
    let persons = rows(&graph, "person");
    let [row] = persons.as_slice() else {
        panic!("one PERSON row, got {}", persons.len());
    };
    assert_eq!(int(row, "gender_concept_id"), Some(GENDER_CONCEPT));
    assert_eq!(int(row, "year_of_birth"), Some(1984));
    assert_eq!(int(row, "race_concept_id"), Some(0));
    Ok(())
}

#[tokio::test]
async fn a_person_mapping_that_omits_a_key_it_could_write_is_refused() -> Result<(), Box<dyn Error>>
{
    // Only a column no OMOCL key writes takes concept 0; `ethnicity_concept_id`
    // has a key, so a mapping that leaves it out cannot write the row.
    let source = gendered(false);
    let (graph, refusals) = person(&[], &[("synthetic_person.yml", &source)]).await?;
    assert_eq!(
        rows(&graph, "person"),
        Vec::<&omop_cdm::graph::row::Row>::new()
    );
    let [refusal] = refusals.as_slice() else {
        panic!("one refusal, got {refusals:?}");
    };
    assert_eq!(refusal.kind, RefusalKind::RequiredColumn);
    assert_eq!(refusal.column, Some("ethnicity_concept_id"));
    Ok(())
}

/// FerroBRIDGE's `Person` mapping for `KDS_Person`.
const KDS_PERSON_DATA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/projects/ferrobridge/kds_person/kds_person_data.yml"
);

/// Runs FerroBRIDGE's KDS `Person` mapping over the person `flat` describes.
async fn kds_person(
    flat: &serde_json::Map<String, Value>,
) -> Result<(RecordGraph, Vec<omocl::engine::RecordRefusal>), Box<dyn Error>> {
    let source = std::fs::read_to_string(KDS_PERSON_DATA)?;
    person_from(flat, &[], &[("kds_person_data.yml", &source)]).await
}

#[tokio::test]
async fn the_kds_person_mapping_writes_the_administrative_gender() -> Result<(), Box<dyn Error>> {
    let (graph, refusals) = kds_person(&flat()?).await?;
    assert!(refusals.is_empty(), "{refusals:?}");
    let persons = rows(&graph, "person");
    let [row] = persons.as_slice() else {
        panic!("one PERSON row, got {}", persons.len());
    };
    // CDM v5.4 `person.gender_concept_id`: "Use the gender or sex value
    // present in the data"; KDS_Person carries it at `items[at0022]`.
    assert_eq!(int(row, "gender_concept_id"), Some(GENDER_CONCEPT));
    assert_eq!(text(row, "gender_source_value"), Some(GENDER_CODE));
    assert_eq!(int(row, "year_of_birth"), Some(1984));
    assert_eq!(int(row, "ethnicity_concept_id"), Some(0));
    assert_eq!(int(row, "race_concept_id"), Some(0));
    Ok(())
}

#[tokio::test]
async fn a_kds_gender_code_with_no_standard_concept_writes_concept_zero_and_its_source()
-> Result<(), Box<dyn Error>> {
    let mut flat = flat()?;
    flat.insert(
        "person/geschlecht/administratives_geschlecht|code".to_owned(),
        Value::from("SYN-GENDER-UNMAPPED"),
    );
    let (graph, refusals) = kds_person(&flat).await?;
    assert!(refusals.is_empty(), "{refusals:?}");
    let persons = rows(&graph, "person");
    let [row] = persons.as_slice() else {
        panic!("one PERSON row, got {}", persons.len());
    };
    // CDM conventions: "If a Standard Concept does not exist or cannot be
    // identified, the Concept with the CONCEPT_ID 0 is used".
    assert_eq!(int(row, "gender_concept_id"), Some(0));
    assert_eq!(
        text(row, "gender_source_value"),
        Some("SYN-GENDER-UNMAPPED")
    );
    Ok(())
}

#[tokio::test]
async fn a_kds_person_with_no_gender_takes_the_literal_concept_zero() -> Result<(), Box<dyn Error>>
{
    let mut flat = flat()?;
    flat.retain(|key, _| !key.starts_with("person/geschlecht/administratives_geschlecht"));
    let (graph, refusals) = kds_person(&flat).await?;
    assert!(refusals.is_empty(), "{refusals:?}");
    let persons = rows(&graph, "person");
    let [row] = persons.as_slice() else {
        panic!("one PERSON row, got {}", persons.len());
    };
    assert_eq!(int(row, "gender_concept_id"), Some(0));
    assert_eq!(row.cell("gender_source_value"), None);
    Ok(())
}
