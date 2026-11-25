// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[etl]` into the ETL job with its queries checked.
//!
//! No specification governs the configuration: our own design.

use omocl::engine::concept::VocabularyAliases;
use std::collections::BTreeMap;

use crate::config::Config;
use crate::config::error::Error;
use crate::config::section::etl::Etl;

/// Returns the checked query `text` holds, refusing it under `key`.
fn query_of(
    key: &str,
    text: &str,
    check: fn(&str) -> Result<crate::aql::CheckedQuery, crate::aql::AqlError>,
) -> Result<crate::aql::CheckedQuery, Error> {
    if text.is_empty() {
        return Err(Error::Missing {
            key: key.to_owned(),
        });
    }
    check(text).map_err(|source| Error::Aql {
        key: key.to_owned(),
        source: Box::new(source),
    })
}

/// Returns the engine's vocabulary aliases with every alias `aliases` names
/// added over them, refusing an empty terminology id, an empty vocabulary and
/// a vocabulary wider than OMOP CDM v5.4 `VOCABULARY.vocabulary_id`.
///
/// No specification governs this table: our own design. An alias only names
/// the `vocabulary_id` a terminology id is looked up under; the concept is
/// still found by SQL over the loaded vocabulary.
fn resolve_aliases(aliases: &BTreeMap<String, String>) -> Result<VocabularyAliases, Error> {
    let mut table = VocabularyAliases::default();
    for (terminology, vocabulary) in aliases {
        if terminology.is_empty() {
            return Err(Error::AliasTerminology {
                key: String::from("etl.vocabulary_aliases"),
            });
        }
        let key = format!("etl.vocabulary_aliases.\"{terminology}\"");
        if vocabulary.is_empty() {
            return Err(Error::Missing { key });
        }
        if let Err(source) = omop_cdm::vocabulary::VocabularyId::new(vocabulary.as_str()) {
            return Err(Error::AliasVocabulary {
                key,
                value: vocabulary.clone(),
                source,
            });
        }
        table = table.with(terminology.as_str(), vocabulary.as_str());
    }
    Ok(table)
}

/// Returns the ETL job `etl` describes, with both queries checked.
fn settings(etl: &Etl) -> Result<crate::config::EtlSettings, Error> {
    // NOTE: no specification governs this: our own design; `etl fhir` reads no CDR, so
    // the composition query is optional here and `etl run` refuses a run without one.
    let compositions = if etl.aql.is_empty() {
        None
    } else {
        Some(query_of("etl.aql", &etl.aql, |text| {
            crate::aql::CheckedQuery::new(crate::aql::Query::Compositions, text)
        })?)
    };
    let page_size =
        crate::cdr::query::PageSize::new(etl.page_size).map_err(|source| Error::PageSize {
            key: String::from("etl.page_size"),
            source,
        })?;
    let visits = etl
        .visits
        .as_ref()
        .map(|visits| {
            Ok::<_, Error>(crate::config::VisitSettings {
                query: query_of("etl.visits.aql", &visits.aql, |text| {
                    crate::aql::CheckedQuery::new(crate::aql::Query::Visits, text)
                })?,
                concepts: omop_cdm::writer::input::VisitConcepts {
                    visit_concept_id: super::required(
                        "etl.visits.visit_concept_id",
                        visits.visit_concept_id,
                    )?,
                    visit_type_concept_id: super::required(
                        "etl.visits.visit_type_concept_id",
                        visits.visit_type_concept_id,
                    )?,
                },
            })
        })
        .transpose()?;
    let deletions = match etl.deletions.as_str() {
        "remove" => crate::config::DeletionPolicy::Remove,
        "report" => crate::config::DeletionPolicy::Report,
        _ => {
            return Err(Error::Deletions {
                key: String::from("etl.deletions"),
                value: etl.deletions.clone(),
            });
        }
    };
    let derived_tables = match etl.derived_tables.as_str() {
        "owned_persons" => omop_cdm::derived::Scope::OwnedPersons,
        "whole_table" => omop_cdm::derived::Scope::WholeTable,
        _ => {
            return Err(Error::DerivedTables {
                key: String::from("etl.derived_tables"),
                value: etl.derived_tables.clone(),
            });
        }
    };
    Ok(crate::config::EtlSettings {
        compositions,
        page_size,
        type_concept_id: super::required("etl.type_concept_id", etl.type_concept_id)?,
        observation_period_type_concept_id: super::required(
            "etl.observation_period_type_concept_id",
            etl.observation_period_type_concept_id,
        )?,
        visits,
        deletions,
        aliases: resolve_aliases(&etl.vocabulary_aliases)?,
        quality_checks: etl.quality_checks,
        derived_tables,
    })
}

/// Returns the ETL job of `[etl]`, when it is present.
///
/// # Errors
///
/// Returns the [`Error`] of the section's first fault.
pub(in crate::config) fn resolve(
    config: &Config,
) -> Result<Option<crate::config::EtlSettings>, Error> {
    config.etl.as_ref().map(settings).transpose()
}
