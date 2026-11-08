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
use crate::facade::search::aql;
use crate::facade::search::aql::Conditions;
use crate::facade::search::aql::OrderKey;
use crate::facade::search::criteria::Criteria;
use crate::facade::search::criteria::CriteriaError;
use crate::facade::search::criteria::Filter;
use crate::facade::search::criteria::Test;
use crate::facade::search::declaration::Parameter;
use crate::facade::search::declaration::ParameterType;
use fhir_search::date::DateValue;
use fhir_search::prefix::Prefix;
use fhir_search::token::{System, Token};
use openehr_query::ast::IdentifiedPath;
use openehr_query::ast::WhereExpr;
use openehr_query::lexer::CompOp;

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
        let alternatives: Vec<WhereExpr> = ehrs
            .iter()
            .map(|ehr| aql::equals(ehr_path(), &conditions.bind(ehr.as_str())))
            .collect();
        let Some(clause) = aql::any(alternatives) else {
            return Ok(None);
        };
        conditions.push(clause);
    }
    for bindings in &resolved.ids {
        let alternatives: Vec<WhereExpr> = bindings
            .iter()
            .filter(|binding| binding.template_id == declared.template_id())
            .filter_map(|binding| {
                let ehr = conditions.bind(binding.ehr_id.as_str());
                // NOTE: openEHR RM Common `OBJECT_VERSION_ID` is `object_id::system::tree`,
                // so `<object_id>::*` (QUERY AQL §3.8.1.1) matches every version of it.
                let versions = conditions.bind(format!("{}::*", binding.versioned_object_uid));
                aql::all([
                    aql::equals(ehr_path(), &ehr),
                    aql::like(aql::attribute_path("v", &["uid", "value"]), &versions),
                ])
            })
            .collect();
        let Some(clause) = aql::any(alternatives) else {
            return Ok(None);
        };
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

/// Returns the path of the EHR's id.
fn ehr_path() -> IdentifiedPath {
    aql::attribute_path("e", &["ehr_id", "value"])
}

/// Returns the `ORDER BY` of `declared` for `criteria`: the `_sort` keys,
/// then the declaration's total order.
///
/// Only a parameter read under the composition sorts, so every key is one
/// value per composition and the rows of one composition stay together.
#[must_use]
pub fn order(declared: &Declared, criteria: &Criteria) -> Vec<OrderKey> {
    let mut keys: Vec<OrderKey> = criteria
        .sort
        .iter()
        .filter_map(|(name, descending)| {
            declared
                .parameters()
                .get(name)
                .filter(|parameter| parameter.sortable())
                .map(|parameter| OrderKey {
                    path: parameter.value_path("c"),
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
///
/// A parameter read under a node binds a fresh alias for this occurrence, so
/// every alternative of the occurrence reads the one node the alias names.
fn condition(
    conditions: &mut Conditions,
    filter: &Filter,
    parameter: &Parameter,
) -> Result<WhereExpr, CriteriaError> {
    let alias = conditions.alias(parameter.node());
    let path = parameter.value_path(&alias);
    let alternatives: Vec<WhereExpr> = match (&filter.test, parameter.kind()) {
        (Test::Token(values), ParameterType::Token) => {
            let system = parameter.system_path(&alias);
            values
                .iter()
                .map(|token| {
                    token_condition(
                        conditions,
                        token,
                        (&path, system.as_ref(), parameter.systems()),
                        &filter.name,
                    )
                })
                .collect::<Result<Vec<WhereExpr>, CriteriaError>>()?
        }
        (Test::Text { matching, values }, ParameterType::String) => values
            .iter()
            .map(
                |text| match crate::facade::search::value::like_pattern(text, *matching) {
                    Ok(Some(pattern)) => Ok(aql::like(path.clone(), &conditions.bind(pattern))),
                    Ok(None) => Ok(aql::equals(path.clone(), &conditions.bind(text.as_str()))),
                    Err(error) => Err(CriteriaError::Unsupported {
                        name: filter.name.clone(),
                        reason: error.to_string(),
                    }),
                },
            )
            .collect::<Result<Vec<WhereExpr>, CriteriaError>>()?,
        (Test::Date(values), ParameterType::Date) => values
            .iter()
            .map(|value| date_condition(conditions, &path, value, &filter.name))
            .collect::<Result<Vec<WhereExpr>, CriteriaError>>()?,
        _ => {
            return Err(CriteriaError::Invalid {
                name: filter.name.clone(),
                reason: String::from("the parameter's declarations disagree on its type"),
            });
        }
    };
    aql::any(alternatives).ok_or_else(|| CriteriaError::Invalid {
        name: filter.name.clone(),
        reason: String::from("the parameter names no value"),
    })
}

/// Returns the condition of one token value.
///
/// The code and the system paths are read under one alias, so both compare
/// parts of one coded value (<https://hl7.org/fhir/R4/search.html#token>).
fn token_condition(
    conditions: &mut Conditions,
    token: &Token,
    paths: (
        &IdentifiedPath,
        Option<&IdentifiedPath>,
        &std::collections::BTreeMap<String, String>,
    ),
    name: &str,
) -> Result<WhereExpr, CriteriaError> {
    let (code_path, system_path, systems) = paths;
    let needs_system = || {
        system_path.cloned().ok_or_else(|| CriteriaError::Unsupported {
            name: name.to_owned(),
            reason: String::from(
                "this parameter is declared over a code alone, so a value naming a system cannot be matched",
            ),
        })
    };
    let mut parts = Vec::new();
    match token.system() {
        System::Any => {}
        System::Absent => parts.push(aql::not(aql::exists(needs_system()?))),
        System::Is(uri) => {
            let path = needs_system()?;
            // NOTE: no specification maps a FHIR system URI to an openEHR terminology id:
            // our own design, the declaration's `systems` table, else the URI verbatim.
            let terminology = systems.get(uri).map_or(uri, String::as_str);
            parts.push(aql::equals(path, &conditions.bind(terminology)));
        }
    }
    if let Some(code) = token.code() {
        parts.push(aql::equals(code_path.clone(), &conditions.bind(code)));
    }
    aql::all(parts).ok_or_else(|| CriteriaError::Invalid {
        name: name.to_owned(),
        reason: String::from("the token names neither a system nor a code"),
    })
}

/// Returns the condition of one date value over `path`.
///
/// The target is the single instant an openEHR `DV_DATE_TIME` records, so the
/// R4 range rules (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.5) reduce
/// to comparisons of that instant with the search range `[low, high)`.
fn date_condition(
    conditions: &mut Conditions,
    path: &IdentifiedPath,
    value: &DateValue,
    name: &str,
) -> Result<WhereExpr, CriteriaError> {
    let range = value.date().range();
    let low = range.low().to_string();
    let high = range.high().map(ToString::to_string);
    let mut at = |op: CompOp, text: &str| aql::compare(path.clone(), op, &conditions.bind(text));
    Ok(match (value.prefix(), high) {
        (Prefix::Eq | Prefix::Ap, Some(high)) => {
            let from = at(CompOp::Ge, &low);
            let to = at(CompOp::Lt, &high);
            WhereExpr::And(Box::new(from), Box::new(to))
        }
        (Prefix::Ne, Some(high)) => {
            let before = at(CompOp::Lt, &low);
            let after = at(CompOp::Ge, &high);
            WhereExpr::Or(Box::new(before), Box::new(after))
        }
        (Prefix::Eq | Prefix::Ap | Prefix::Ge, None) | (Prefix::Ge, Some(_)) => {
            at(CompOp::Ge, &low)
        }
        (Prefix::Ne | Prefix::Lt | Prefix::Eb, _) => at(CompOp::Lt, &low),
        (Prefix::Gt | Prefix::Sa, Some(high)) => at(CompOp::Ge, &high),
        (Prefix::Le, Some(high)) => at(CompOp::Lt, &high),
        (Prefix::Gt | Prefix::Sa | Prefix::Le, None) => {
            return Err(CriteriaError::Unsupported {
                name: name.to_owned(),
                reason: String::from(
                    "the range of this date runs past the year 9999, and the CDR compares no date beyond it",
                ),
            });
        }
    })
}
