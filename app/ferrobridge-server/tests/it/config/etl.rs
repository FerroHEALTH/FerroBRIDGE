// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[cdm]` and `[etl]` configuration: the schema names, the person
//! policy, and the two queries checked when the configuration loads.

use ferrobridge_server::config::Config;
use ferrobridge_server::config::error::Error;
use ferrobridge_server::etl::aql::AqlError;
use ferrobridge_server::etl::deletions::DeletionPolicy;
use std::collections::BTreeMap;

/// An `[etl]` section with every required key and a composition query that
/// carries the four projections and an order.
const ETL: &str = r#"
[etl]
aql = "SELECT e/ehr_id/value AS ehr_id, vo/uid/value AS versioned_object_uid, v/uid/value AS version_uid, c AS composition FROM EHR e CONTAINS VERSIONED_OBJECT vo CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c ORDER BY v/uid/value"
type_concept_id = 32817
observation_period_type_concept_id = 32880
"#;

/// A visit query with the four projections.
const VISITS: &str = "SELECT e/ehr_id/value AS ehr_id, c/name/value AS visit_source, c/context/start_time/value AS visit_start, c/context/end_time/value AS visit_end FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c ORDER BY c/context/start_time/value";

/// Resolves `text`, returning the refusal.
fn refusal(text: &str) -> Result<Error, Box<dyn std::error::Error>> {
    match Config::from_sources(Some(text), &BTreeMap::new())?.resolve() {
        Ok(_) => Err("the configuration resolved".into()),
        Err(error) => Ok(error),
    }
}

#[test]
fn the_cdm_section_defaults_its_schemas_and_policy() -> Result<(), Box<dyn std::error::Error>> {
    let text = "[cdm]\nurl = \"postgres://db.invalid/cdm?sslmode=disable\"\n";
    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
    let cdm = settings.cdm.as_ref().ok_or("the CDM lane is on")?;
    assert_eq!("cdm", cdm.schema.as_str());
    assert_eq!("ferrobridge", cdm.bridge_schema.as_str());
    assert_eq!(
        omop_cdm::writer::input::PersonPolicy::CreateOnFirstSight,
        cdm.person_policy
    );
    Ok(())
}

#[test]
fn a_schema_that_is_no_identifier_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(
        "[cdm]\nurl = \"postgres://db.invalid/cdm?sslmode=disable\"\nbridge_schema = \"bridge; drop\"\n",
    )?;
    match &error {
        Error::Schema { key, .. } => assert_eq!("cdm.bridge_schema", key),
        other => return Err(format!("expected a schema error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_unknown_person_policy_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(
        "[cdm]\nurl = \"postgres://db.invalid/cdm?sslmode=disable\"\nperson_policy = \"merge\"\n",
    )?;
    assert!(matches!(error, Error::PersonPolicy { .. }), "{error:?}");
    Ok(())
}

#[test]
fn an_etl_section_resolves_with_its_query_checked() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(ETL), &BTreeMap::new())?.resolve()?;
    let etl = settings.etl.as_ref().ok_or("the ETL is configured")?;
    assert_eq!(32817, etl.type_concept_id);
    assert_eq!(32880, etl.observation_period_type_concept_id);
    assert_eq!(
        Some(2),
        etl.compositions
            .as_ref()
            .and_then(|query| query.column("version_uid"))
    );
    assert_eq!(100, etl.page_size.rows(), "the default page");
    assert!(etl.visits.is_none());
    Ok(())
}

#[test]
fn a_composition_query_without_its_projections_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    for (alias, projection) in [
        ("AS ehr_id", "ehr_id"),
        ("AS version_uid", "version_uid"),
        ("AS composition", "composition"),
    ] {
        let error = refusal(&ETL.replace(alias, "AS other"))?;
        match &error {
            Error::Aql { key, source } => {
                assert_eq!("etl.aql", key);
                assert!(
                    matches!(**source, AqlError::Missing { projection: missing, .. } if missing == projection),
                    "{projection}: {source:?}"
                );
            }
            other => return Err(format!("expected an AQL refusal, got {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn a_composition_query_without_the_versioned_object_uid_boots()
-> Result<(), Box<dyn std::error::Error>> {
    let text = ETL.replace("vo/uid/value AS versioned_object_uid, ", "");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let etl = settings.etl.as_ref().ok_or("the ETL is configured")?;
    assert_eq!(
        None,
        etl.compositions
            .as_ref()
            .and_then(|query| query.column("versioned_object_uid"))
    );
    assert_eq!(
        Some(1),
        etl.compositions
            .as_ref()
            .and_then(|query| query.column("version_uid"))
    );
    Ok(())
}

#[test]
fn a_composition_query_without_an_order_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>>
{
    let error = refusal(&ETL.replace(" ORDER BY v/uid/value", ""))?;
    match &error {
        Error::Aql { source, .. } => {
            assert!(matches!(**source, AqlError::Unordered { .. }), "{source:?}");
        }
        other => return Err(format!("expected an AQL refusal, got {other:?}").into()),
    }
    Ok(())
}

// NOTE: QUERY AQL 1.1.0 §6 leaves a query's version scope open without
// LATEST_VERSION, and a deletion is a version with Void data (RM Common §6.2.5).
#[test]
fn a_composition_query_over_every_version_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>>
{
    let error = refusal(&ETL.replace("VERSION v[LATEST_VERSION]", "VERSION v"))?;
    match &error {
        Error::Aql { key, source } => {
            assert_eq!("etl.aql", key);
            assert!(matches!(**source, AqlError::Unscoped { .. }), "{source:?}");
        }
        other => return Err(format!("expected an AQL refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_visit_query_over_every_version_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let query = VISITS.replace("CONTAINS VERSION v[LATEST_VERSION] ", "");
    let error = refusal(&format!(
        "{ETL}\n[etl.visits]\naql = \"{query}\"\nvisit_concept_id = 9201\nvisit_type_concept_id = 32817\n"
    ))?;
    match &error {
        Error::Aql { key, source } => {
            assert_eq!("etl.visits.aql", key);
            assert!(matches!(**source, AqlError::Unscoped { .. }), "{source:?}");
        }
        other => return Err(format!("expected an AQL refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_visit_query_naming_since_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let query = VISITS.replace(
        " ORDER BY",
        " WHERE v/commit_audit/time_committed/value > $since ORDER BY",
    );
    let error = refusal(&format!(
        "{ETL}\n[etl.visits]\naql = \"{query}\"\nvisit_concept_id = 9201\nvisit_type_concept_id = 32817\n"
    ))?;
    match &error {
        Error::Aql { key, source } => {
            assert_eq!("etl.visits.aql", key);
            assert!(matches!(**source, AqlError::Windowed { .. }), "{source:?}");
            assert!(source.to_string().contains("$since"), "{source}");
        }
        other => return Err(format!("expected an AQL refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_visit_query_naming_no_since_boots() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "{ETL}\n[etl.visits]\naql = \"{VISITS}\"\nvisit_concept_id = 9201\nvisit_type_concept_id = 32817\n"
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let etl = settings.etl.as_ref().ok_or("the ETL is configured")?;
    let visits = etl.visits.as_ref().ok_or("the visits are configured")?;
    assert_eq!(Some(0), visits.query.column("ehr_id"));
    Ok(())
}

#[test]
fn a_visit_query_that_does_not_parse_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!(
        "{ETL}\n[etl.visits]\naql = \"SELECT FROM\"\nvisit_concept_id = 9201\nvisit_type_concept_id = 32817\n"
    ))?;
    match &error {
        Error::Aql { key, source } => {
            assert_eq!("etl.visits.aql", key);
            assert!(matches!(**source, AqlError::Parse { .. }), "{source:?}");
        }
        other => return Err(format!("expected an AQL refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_visit_query_without_its_four_projections_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    for (alias, projection) in [
        ("AS ehr_id", "ehr_id"),
        ("AS visit_source", "visit_source"),
        ("AS visit_start", "visit_start"),
        ("AS visit_end", "visit_end"),
    ] {
        let query = VISITS.replace(alias, "AS other");
        let error = refusal(&format!(
            "{ETL}\n[etl.visits]\naql = \"{query}\"\nvisit_concept_id = 9201\nvisit_type_concept_id = 32817\n"
        ))?;
        match &error {
            Error::Aql { source, .. } => assert!(
                matches!(**source, AqlError::Missing { projection: missing, .. } if missing == projection),
                "{projection}: {source:?}"
            ),
            other => return Err(format!("expected an AQL refusal, got {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn a_visit_section_without_its_concepts_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>>
{
    let error = refusal(&format!("{ETL}\n[etl.visits]\naql = \"{VISITS}\"\n"))?;
    match &error {
        Error::Missing { key } => assert_eq!("etl.visits.visit_concept_id", key),
        other => return Err(format!("expected a missing key, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_etl_section_without_a_type_concept_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>>
{
    let error = refusal(&ETL.replace("\ntype_concept_id = 32817", ""))?;
    match &error {
        Error::Missing { key } => assert_eq!("etl.type_concept_id", key),
        other => return Err(format!("expected a missing key, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_page_size_of_zero_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!("{ETL}page_size = 0\n"))?;
    assert!(matches!(error, Error::PageSize { .. }), "{error:?}");
    Ok(())
}

#[test]
fn the_deletion_policy_defaults_to_remove() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(ETL), &BTreeMap::new())?.resolve()?;
    let etl = settings.etl.as_ref().ok_or("the ETL is configured")?;
    assert_eq!(DeletionPolicy::Remove, etl.deletions);
    Ok(())
}

#[test]
fn the_report_deletion_policy_resolves() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!("{ETL}deletions = \"report\"\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let etl = settings.etl.as_ref().ok_or("the ETL is configured")?;
    assert_eq!(DeletionPolicy::Report, etl.deletions);
    Ok(())
}

#[test]
fn an_unknown_deletion_policy_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!("{ETL}deletions = \"ignore\"\n"))?;
    match &error {
        Error::Deletions { key, value } => {
            assert_eq!("etl.deletions", key);
            assert_eq!("ignore", value);
        }
        other => return Err(format!("expected a deletion policy refusal, got {other:?}").into()),
    }
    Ok(())
}

/// Returns `ETL` with `table` under `[etl.vocabulary_aliases]`.
fn with_aliases(table: &str) -> String {
    format!("{ETL}[etl.vocabulary_aliases]\n{table}\n")
}

#[test]
fn the_default_aliases_read_the_terminologies_the_laboratory_compositions_name()
-> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(ETL), &BTreeMap::new())?.resolve()?;
    let etl = settings.etl.as_ref().ok_or("the ETL is configured")?;
    assert_eq!("LOINC", etl.aliases.vocabulary_of("LOINC"));
    assert_eq!("SNOMED", etl.aliases.vocabulary_of("SNOMED-CT"));
    assert_eq!("FB-SOURCE", etl.aliases.vocabulary_of("FB-SOURCE"));
    Ok(())
}

#[test]
fn a_table_of_aliases_resolves_over_the_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let text = with_aliases(
        "\"http://terminology.hl7.org/CodeSystem/v2-0001\" = \"Gender\"\n\
         \"urn:oid:2.16.840.1.113883.6.1\" = \"LOINC\"\n\
         \"SNOMED-CT\" = \"FB-SNOMED\"",
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let aliases = &settings
        .etl
        .as_ref()
        .ok_or("the ETL is configured")?
        .aliases;
    assert_eq!(
        "Gender",
        aliases.vocabulary_of("http://terminology.hl7.org/CodeSystem/v2-0001")
    );
    assert_eq!(
        "LOINC",
        aliases.vocabulary_of("urn:oid:2.16.840.1.113883.6.1")
    );
    assert_eq!("FB-SNOMED", aliases.vocabulary_of("SNOMED-CT"));
    assert_eq!("LOINC", aliases.vocabulary_of("LOINC"), "a default stays");
    Ok(())
}

#[test]
fn an_alias_with_an_empty_terminology_id_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>>
{
    let error = refusal(&with_aliases("\"\" = \"Gender\""))?;
    match &error {
        Error::AliasTerminology { key } => assert_eq!("etl.vocabulary_aliases", key),
        other => return Err(format!("expected an alias refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_alias_with_an_empty_vocabulary_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&with_aliases("\"urn:oid:2.16.840.1.113883.6.1\" = \"\""))?;
    match &error {
        Error::Missing { key } => {
            assert_eq!(
                "etl.vocabulary_aliases.\"urn:oid:2.16.840.1.113883.6.1\"",
                key
            );
        }
        other => return Err(format!("expected a missing vocabulary, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_alias_to_a_vocabulary_wider_than_the_cdm_column_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    // OMOP CDM v5.4 `VOCABULARY.vocabulary_id` is `varchar(20)`.
    let error = refusal(&with_aliases("\"LOCAL\" = \"A-VOCABULARY-OF-21-CH\""))?;
    match &error {
        Error::AliasVocabulary { key, value, .. } => {
            assert_eq!("etl.vocabulary_aliases.\"LOCAL\"", key);
            assert_eq!("A-VOCABULARY-OF-21-CH", value);
        }
        other => return Err(format!("expected an alias refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_alias_that_is_no_string_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = Config::from_sources(Some(&with_aliases("\"LOCAL\" = 7")), &BTreeMap::new())
        .err()
        .ok_or("an alias of 7 resolved")?;
    let Error::Parse { source } = &error else {
        return Err(format!("expected a parse error, got {error:?}").into());
    };
    assert!(source.to_string().contains("LOCAL"), "{source}");
    Ok(())
}

#[test]
fn the_quality_checks_are_off_by_default_and_switch_on() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(ETL), &BTreeMap::new())?.resolve()?;
    let etl = settings.etl.as_ref().ok_or("the ETL is configured")?;
    assert!(!etl.quality_checks);
    let text = format!("{ETL}quality_checks = true\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let etl = settings.etl.as_ref().ok_or("the ETL is configured")?;
    assert!(etl.quality_checks);
    Ok(())
}
