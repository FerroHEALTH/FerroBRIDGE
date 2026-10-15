// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query one declaration runs for one request.
//!
//! The subject parameters and `_id` resolve through the identity map before
//! any query is built: a subject the map does not know and an id it does not
//! know can match nothing, so the declaration answers no rows without a CDR
//! call (<https://hl7.org/fhir/R4/search.html>, §3.1.1.3: "where the issue is
//! a logical condition (e.g. unknown subject or code), the server SHOULD
//! process the search … with the result of returning an empty search set").
//! Every other parameter becomes one `WHERE` condition over the declaration's
//! paths, its values bound as AQL parameters.

use crate::cdr::ids::EhrId;
use crate::facade::identity::record::CompositionBinding;
use crate::facade::search::Declared;
use crate::facade::search::aql::Conditions;
use crate::facade::search::aql::OrderKey;
use crate::facade::search::criteria::Criteria;
use crate::facade::search::criteria::CriteriaError;
use crate::facade::search::criteria::Filter;
use crate::facade::search::criteria::Test;
use crate::facade::search::declaration::Parameter;
use crate::facade::search::value::DateValue;
use crate::facade::search::value::Prefix;
use crate::facade::search::value::System;
use crate::facade::search::value::Token;

/// What the identity map answered for the subject parameters and `_id`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved {
    /// Each subject parameter occurrence, the EHRs its subjects are bound to.
    pub ehrs: Vec<Vec<EhrId>>,
    /// Each `_id` occurrence, the compositions its ids are bound to.
    pub ids: Vec<Vec<CompositionBinding>>,
}

/// Returns the conditions `declared` runs for `criteria`, or `None` when the
/// identity map already shows it can match nothing.
///
/// # Errors
///
/// Returns [`CriteriaError::Unsupported`] for a token value naming a system
/// on a parameter whose declaration states no system path.
pub fn conditions(
    declared: &Declared,
    criteria: &Criteria,
    resolved: &Resolved,
) -> Result<Option<Conditions>, CriteriaError> {
    let mut conditions = Conditions::new(declared.template_id());
    for ehrs in &resolved.ehrs {
        if ehrs.is_empty() {
            return Ok(None);
        }
        let clause = ehrs
            .iter()
            .map(|ehr| format!("e/ehr_id/value = {}", conditions.bind(ehr.as_str())))
            .collect::<Vec<String>>()
            .join(" OR ");
        conditions.push(clause);
    }
    for bindings in &resolved.ids {
        let mine: Vec<&CompositionBinding> = bindings
            .iter()
            .filter(|binding| binding.template_id == declared.template_id())
            .collect();
        if mine.is_empty() {
            return Ok(None);
        }
        let clause = mine
            .iter()
            .map(|binding| {
                let ehr = conditions.bind(binding.ehr_id.as_str());
                // NOTE: openEHR RM Common `OBJECT_VERSION_ID` is `object_id::system::tree`,
                // so `<object_id>::*` (QUERY AQL §3.8.1.1) matches every version of it.
                let versions = conditions.bind(format!("{}::*", binding.versioned_object_uid));
                format!("(e/ehr_id/value = {ehr} AND v/uid/value LIKE {versions})")
            })
            .collect::<Vec<String>>()
            .join(" OR ");
        conditions.push(clause);
    }
    for filter in &criteria.filters {
        let Some(parameter) = declared.parameters().get(&filter.name) else {
            continue;
        };
        let clause = condition(&mut conditions, filter, parameter)?;
        conditions.push(clause);
    }
    Ok(Some(conditions))
}

/// Returns the `ORDER BY` of `declared` for `criteria`: the `_sort` keys,
/// then the declaration's total order.
#[must_use]
pub fn order(declared: &Declared, criteria: &Criteria) -> Vec<OrderKey> {
    let mut keys: Vec<OrderKey> = criteria
        .sort
        .iter()
        .filter_map(|(name, descending)| {
            declared.parameters().get(name).map(|parameter| OrderKey {
                path: parameter.sort_path().to_owned(),
                descending: *descending,
            })
        })
        .collect();
    keys.extend(declared.order().iter().map(|path| OrderKey {
        path: path.clone(),
        descending: false,
    }));
    keys
}

/// Returns the condition one filter writes over its parameter's paths.
fn condition(
    conditions: &mut Conditions,
    filter: &Filter,
    parameter: &Parameter,
) -> Result<String, CriteriaError> {
    let alternatives: Vec<String> = match (&filter.test, parameter) {
        (
            Test::Token(values),
            Parameter::Token {
                code,
                system,
                systems,
            },
        ) => values
            .iter()
            .map(|token| {
                token_condition(
                    conditions,
                    token,
                    (code, system.as_deref(), systems),
                    &filter.name,
                )
            })
            .collect::<Result<Vec<String>, CriteriaError>>()?,
        (Test::Text { matching, values }, Parameter::String { path }) => values
            .iter()
            .map(
                |text| match crate::facade::search::value::like_pattern(text, *matching) {
                    Ok(Some(pattern)) => Ok(format!("{path} LIKE {}", conditions.bind(pattern))),
                    Ok(None) => Ok(format!("{path} = {}", conditions.bind(text.as_str()))),
                    Err(error) => Err(CriteriaError::Unsupported {
                        name: filter.name.clone(),
                        reason: error.to_string(),
                    }),
                },
            )
            .collect::<Result<Vec<String>, CriteriaError>>()?,
        (Test::Date(values), Parameter::Date { path }) => values
            .iter()
            .map(|value| date_condition(conditions, path, value))
            .collect(),
        _ => {
            return Err(CriteriaError::Invalid {
                name: filter.name.clone(),
                reason: String::from("the parameter's declarations disagree on its type"),
            });
        }
    };
    Ok(alternatives
        .iter()
        .map(|alternative| format!("({alternative})"))
        .collect::<Vec<String>>()
        .join(" OR "))
}

/// Returns the condition of one token value.
fn token_condition(
    conditions: &mut Conditions,
    token: &Token,
    paths: (
        &str,
        Option<&str>,
        &std::collections::BTreeMap<String, String>,
    ),
    name: &str,
) -> Result<String, CriteriaError> {
    let (code_path, system_path, systems) = paths;
    let needs_system = || {
        system_path.ok_or_else(|| CriteriaError::Unsupported {
            name: name.to_owned(),
            reason: String::from(
                "this parameter is declared over a code alone, so a value naming a system cannot be matched",
            ),
        })
    };
    let mut parts = Vec::new();
    match token.system {
        System::Any => {}
        System::Absent => parts.push(format!("NOT EXISTS {}", needs_system()?)),
        System::Is(ref uri) => {
            let path = needs_system()?;
            // NOTE: no specification maps a FHIR system URI to an openEHR terminology id:
            // our own design, the declaration's `systems` table, else the URI verbatim.
            let terminology = systems.get(uri).map_or(uri.as_str(), String::as_str);
            parts.push(format!("{path} = {}", conditions.bind(terminology)));
        }
    }
    if let Some(ref code) = token.code {
        parts.push(format!("{code_path} = {}", conditions.bind(code.as_str())));
    }
    Ok(parts.join(" AND "))
}

/// Returns the condition of one date value over `path`.
///
/// The target is the single instant an openEHR `DV_DATE_TIME` records, so the
/// R4 range rules (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.5) reduce
/// to comparisons of that instant with the search range `[low, high)`.
fn date_condition(conditions: &mut Conditions, path: &str, value: &DateValue) -> String {
    match value.prefix {
        Prefix::Equal => {
            let low = conditions.bind(value.low.as_str());
            let high = conditions.bind(value.high.as_str());
            format!("{path} >= {low} AND {path} < {high}")
        }
        Prefix::NotEqual => {
            let low = conditions.bind(value.low.as_str());
            let high = conditions.bind(value.high.as_str());
            format!("{path} < {low} OR {path} >= {high}")
        }
        Prefix::Less | Prefix::EndsBefore => {
            format!("{path} < {}", conditions.bind(value.low.as_str()))
        }
        Prefix::GreaterOrEqual => format!("{path} >= {}", conditions.bind(value.low.as_str())),
        Prefix::Greater | Prefix::StartsAfter => {
            format!("{path} >= {}", conditions.bind(value.high.as_str()))
        }
        Prefix::LessOrEqual => format!("{path} < {}", conditions.bind(value.high.as_str())),
    }
}
