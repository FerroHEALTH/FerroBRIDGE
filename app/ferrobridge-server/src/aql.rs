// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configured AQL, parsed and checked when the configuration loads.
//!
//! Each query names what the runner reads by its `AS` alias, so a result row
//! is read by position against the parsed `SELECT` list. A query that does
//! not parse as AQL 1.1.0 (the `openehr-query` grammar), lacks a projection
//! the runner reads, has no `ORDER BY` ending in the version uid, or carries
//! a `LIMIT` of its own is refused before any call to the CDR. The runner
//! pages with the `offset` and `fetch` members of ITS-REST 1.1.0
//! (`query-codegen.openapi.yaml`, `components.parameters.offset`), which name
//! row numbers, and row numbers only stay put across pages under a total
//! order. Every version of one contribution carries the contribution's
//! `time_committed` (openEHR RM Common 1.1.0 §6, Contributions), so a commit
//! instant alone leaves ties, and the last key is the `uid/value` of the
//! `VERSION [LATEST_VERSION]` variable, one version per versioned object and
//! so one per row. No specification governs the aliases or the ordering
//! rule: our own design.
//!
//! Every `COMPOSITION` a query reads sits below `VERSION [LATEST_VERSION]`.
//! AQL 1.1.0 names `LATEST_VERSION` and `ALL_VERSIONS` (the `versionPredicate`
//! rule, QUERY AQL §6) and leaves the scope of a query without a `VERSION`
//! open, while a deleted composition is a version whose data is Void (openEHR
//! RM Common 1.1.0 §6.2.5); under that scope neither a deleted composition
//! nor a superseded version reaches a mapping or a visit.

use openehr_query::ast::{ClassExprOperand, ContainsExpr, IdentifiedPath, VersionPredicate};
use openehr_query::lexer::{self, Token};
use openehr_query::parser::{self, ParseError};
use std::collections::BTreeMap;

/// The projections the composition query must alias, in no particular order.
pub const COMPOSITION_PROJECTIONS: [&str; 3] = ["ehr_id", "version_uid", "composition"];

/// The projections the composition query may alias.
///
/// A `versioned_object_uid` the query selects is checked against the
/// `version_uid`; one it does not select is read from the `version_uid`,
/// whose `object_id` part names the versioned object (openEHR RM Common 1.1.0,
/// `OBJECT_VERSION_ID`).
pub const OPTIONAL_COMPOSITION_PROJECTIONS: [&str; 1] = ["versioned_object_uid"];

/// The projections the visit query must alias, in no particular order.
pub const VISIT_PROJECTIONS: [&str; 4] = ["ehr_id", "visit_source", "visit_start", "visit_end"];

/// The query parameter `etl run --since` binds.
pub const SINCE_PARAMETER: &str = "since";

/// A configured query the runner refuses to send.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AqlError {
    /// The text is not AQL.
    #[error("the {query} does not parse as AQL")]
    Parse {
        /// Which query.
        query: &'static str,
        /// What the parser reported.
        #[source]
        source: ParseError,
    },
    /// A projection the runner reads is not aliased in the `SELECT` list.
    #[error("the {query} selects nothing `AS {projection}`, which the runner reads")]
    Missing {
        /// Which query.
        query: &'static str,
        /// The alias that is missing.
        projection: &'static str,
    },
    /// Two columns carry one alias.
    #[error("the {query} selects two columns `AS {alias}`")]
    Duplicate {
        /// Which query.
        query: &'static str,
        /// The alias.
        alias: String,
    },
    /// The query has no `ORDER BY`, so its pages have no fixed rows.
    #[error("the {query} has no ORDER BY, so paging it by offset could skip or repeat rows")]
    Unordered {
        /// Which query.
        query: &'static str,
    },
    /// The `ORDER BY` does not end in the `uid/value` of a
    /// `VERSION [LATEST_VERSION]` variable, so two rows can tie.
    #[error(
        "the {query}'s ORDER BY does not end in the uid/value of its VERSION [LATEST_VERSION] variable (`v/uid/value`), so rows that tie on the other keys could be skipped or repeated across pages"
    )]
    Unkeyed {
        /// Which query.
        query: &'static str,
    },
    /// The query limits its own rows, which the runner pages.
    #[error("the {query} carries a LIMIT; the runner pages the result itself")]
    Limited {
        /// Which query.
        query: &'static str,
    },
    /// A `COMPOSITION` of the `FROM` sits below no `VERSION [LATEST_VERSION]`,
    /// or the `FROM` reads none.
    #[error(
        "the {query} reads no COMPOSITION below VERSION v[LATEST_VERSION]; without that scope a deleted or superseded composition can come back"
    )]
    Unscoped {
        /// Which query.
        query: &'static str,
    },
    /// The visit query names `$since`, which the runner never binds for it.
    #[error(
        "the {query} names $since; the runner binds --since in the composition query only and reads every visit, so the parameter would be sent unbound"
    )]
    Windowed {
        /// Which query.
        query: &'static str,
    },
}

/// A query that parsed and carries every projection its reader needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedQuery {
    text: String,
    columns: BTreeMap<&'static str, usize>,
    parameters: Vec<String>,
}

impl CheckedQuery {
    /// Checks `text` as the composition query: `ehr_id`, `version_uid` and
    /// the whole `composition`, with `versioned_object_uid` when it selects
    /// one.
    ///
    /// # Errors
    ///
    /// Returns [`AqlError`] naming the first fault.
    pub fn compositions(text: &str) -> Result<Self, AqlError> {
        Self::check(
            "composition query",
            text,
            &COMPOSITION_PROJECTIONS,
            &OPTIONAL_COMPOSITION_PROJECTIONS,
        )
    }

    /// Checks `text` as the visit query: `ehr_id`, `visit_source`,
    /// `visit_start` and `visit_end`, naming no `$since`.
    ///
    /// The runner reads every visit the deployment derives and never binds
    /// `--since` to this query, so a `$since` in it would reach the CDR
    /// unbound. No specification governs this: our own design.
    ///
    /// # Errors
    ///
    /// Returns [`AqlError`] naming the first fault, [`AqlError::Windowed`]
    /// when the query names `$since`.
    pub fn visits(text: &str) -> Result<Self, AqlError> {
        const QUERY: &str = "visit query";
        let checked = Self::check(QUERY, text, &VISIT_PROJECTIONS, &[])?;
        if checked.names_parameter(SINCE_PARAMETER) {
            return Err(AqlError::Windowed { query: QUERY });
        }
        Ok(checked)
    }

    /// Parses `text` and finds each of `projections`, and each of `optional`
    /// it selects, in its `SELECT` list.
    fn check(
        query: &'static str,
        text: &str,
        projections: &[&'static str],
        optional: &[&'static str],
    ) -> Result<Self, AqlError> {
        let parsed = parser::parse_str(text).map_err(|source| AqlError::Parse { query, source })?;
        if parsed.order_by.is_empty() {
            return Err(AqlError::Unordered { query });
        }
        if parsed.limit.is_some() {
            return Err(AqlError::Limited { query });
        }
        let mut aliases: BTreeMap<&str, usize> = BTreeMap::new();
        for (position, column) in parsed.select.columns.iter().enumerate() {
            if let Some(alias) = column.alias.as_deref()
                && aliases.insert(alias, position).is_some()
            {
                return Err(AqlError::Duplicate {
                    query,
                    alias: alias.to_owned(),
                });
            }
        }
        let mut columns = BTreeMap::new();
        for projection in projections {
            let position = aliases
                .get(projection)
                .copied()
                .ok_or(AqlError::Missing { query, projection })?;
            columns.insert(*projection, position);
        }
        for projection in optional {
            if let Some(position) = aliases.get(projection).copied() {
                columns.insert(*projection, position);
            }
        }
        if latest_scoped(&parsed.from, false) != Some(true) {
            return Err(AqlError::Unscoped { query });
        }
        let mut versions = Vec::new();
        latest_versions(&parsed.from, &mut versions);
        let keyed = parsed
            .order_by
            .last()
            .is_some_and(|last| version_uid(&last.path, &versions));
        if !keyed {
            return Err(AqlError::Unkeyed { query });
        }
        let parameters = lexer::lex(text)
            .map_err(|source| AqlError::Parse {
                query,
                source: ParseError::Lex(source),
            })?
            .into_iter()
            .filter_map(|token| match token {
                Token::Parameter(name) => name.strip_prefix('$').map(str::to_owned),
                _ => None,
            })
            .collect();
        Ok(Self {
            text: text.to_owned(),
            columns,
            parameters,
        })
    }

    /// Returns the query text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the position of `projection` in a result row, when the query
    /// carries it.
    #[must_use]
    pub fn column(&self, projection: &str) -> Option<usize> {
        self.columns.get(projection).copied()
    }

    /// Returns whether the query names the parameter `$name`.
    #[must_use]
    pub fn names_parameter(&self, name: &str) -> bool {
        self.parameters.iter().any(|parameter| parameter == name)
    }
}

/// Returns whether every `COMPOSITION` that `expr` binds sits below a
/// `VERSION [LATEST_VERSION]`, or `None` when it binds none.
///
/// `latest` says whether the nearest enclosing `VERSION` is `LATEST_VERSION`.
/// A `NOT CONTAINS` branch binds no row, so it is not read.
fn latest_scoped(expr: &ContainsExpr, latest: bool) -> Option<bool> {
    match expr {
        ContainsExpr::Contained { operand, contains } => {
            let (own, inner) = match operand {
                ClassExprOperand::Class { rm_type, .. } => (
                    rm_type
                        .eq_ignore_ascii_case("COMPOSITION")
                        .then_some(latest),
                    latest,
                ),
                ClassExprOperand::Version { predicate, .. } => {
                    (None, matches!(predicate, Some(VersionPredicate::Latest)))
                }
            };
            let below = contains
                .as_deref()
                .filter(|constraint| !constraint.negated)
                .and_then(|constraint| latest_scoped(&constraint.expr, inner));
            both(own, below)
        }
        ContainsExpr::And(left, right) | ContainsExpr::Or(left, right) => {
            both(latest_scoped(left, latest), latest_scoped(right, latest))
        }
    }
}

/// Collects the variable of every `VERSION [LATEST_VERSION]` that `expr`
/// binds, outside a `NOT CONTAINS` branch.
fn latest_versions<'a>(expr: &'a ContainsExpr, into: &mut Vec<&'a str>) {
    match expr {
        ContainsExpr::Contained { operand, contains } => {
            if let ClassExprOperand::Version {
                variable: Some(variable),
                predicate: Some(VersionPredicate::Latest),
            } = operand
            {
                into.push(variable);
            }
            if let Some(constraint) = contains.as_deref().filter(|constraint| !constraint.negated) {
                latest_versions(&constraint.expr, into);
            }
        }
        ContainsExpr::And(left, right) | ContainsExpr::Or(left, right) => {
            latest_versions(left, into);
            latest_versions(right, into);
        }
    }
}

/// Returns whether `path` is `<v>/uid/value` for one of `versions`.
fn version_uid(path: &IdentifiedPath, versions: &[&str]) -> bool {
    let parts = path.path.as_ref().map(|path| path.parts.as_slice());
    let uid_value = matches!(
        parts,
        Some([uid, value])
            if uid.name == "uid" && value.name == "value"
                && uid.predicate.is_none() && value.predicate.is_none()
    );
    uid_value && path.predicate.is_none() && versions.contains(&path.root.as_str())
}

/// Joins two answers of [`latest_scoped`].
fn both(left: Option<bool>, right: Option<bool>) -> Option<bool> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left && right),
        (one, None) | (None, one) => one,
    }
}

#[cfg(test)]
mod tests {
    use super::{AqlError, CheckedQuery, SINCE_PARAMETER};

    /// A composition query with the four projections.
    const COMPOSITIONS: &str = "SELECT e/ehr_id/value AS ehr_id, \
        v/uid/value AS version_uid, c AS composition, \
        vo/uid/value AS versioned_object_uid \
        FROM EHR e CONTAINS VERSIONED_OBJECT vo CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
        ORDER BY v/commit_audit/time_committed/value, v/uid/value";

    #[test]
    fn the_four_projections_are_found_by_alias() {
        let query = CheckedQuery::compositions(COMPOSITIONS).expect("the query checks");
        assert_eq!(Some(0), query.column("ehr_id"));
        assert_eq!(Some(3), query.column("versioned_object_uid"));
        assert_eq!(Some(2), query.column("composition"));
        assert!(!query.names_parameter(SINCE_PARAMETER));
    }

    #[test]
    fn a_query_that_does_not_parse_is_refused() {
        let error = CheckedQuery::compositions("SELECT FROM WHERE").expect_err("not AQL");
        assert!(matches!(error, AqlError::Parse { .. }), "{error:?}");
    }

    #[test]
    fn a_missing_projection_is_refused_by_name() {
        let error = CheckedQuery::compositions(
            "SELECT e/ehr_id/value AS ehr_id, c AS composition FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c ORDER BY e/ehr_id/value",
        )
        .expect_err("the version_uid is missing");
        assert!(
            matches!(
                error,
                AqlError::Missing {
                    projection: "version_uid",
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_query_without_the_versioned_object_uid_checks_and_reads_none() {
        let query = CheckedQuery::compositions(
            "SELECT e/ehr_id/value AS ehr_id, v/uid/value AS version_uid, c AS composition \
             FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c ORDER BY v/uid/value",
        )
        .expect("the query checks");
        assert_eq!(Some(1), query.column("version_uid"));
        assert_eq!(None, query.column("versioned_object_uid"));
    }

    /// Returns the composition query over `from`, with the three projections.
    fn over(from: &str) -> String {
        format!(
            "SELECT e/ehr_id/value AS ehr_id, v/uid/value AS version_uid, c AS composition \
             FROM {from} ORDER BY v/uid/value"
        )
    }

    // NOTE: QUERY AQL 1.1.0 §6 (`versionPredicate`) leaves the scope of a query
    // without LATEST_VERSION open, and a deletion is a version with Void data
    // (openEHR RM Common 1.1.0 §6.2.5), so each of these is refused.
    #[test]
    fn a_composition_below_no_latest_version_scope_is_refused() {
        for from in [
            "EHR e CONTAINS COMPOSITION c",
            "EHR e CONTAINS VERSION v CONTAINS COMPOSITION c",
            "EHR e CONTAINS VERSION v[ALL_VERSIONS] CONTAINS COMPOSITION c",
            "EHR e CONTAINS VERSIONED_OBJECT vo CONTAINS VERSION v CONTAINS COMPOSITION c",
            "EHR e CONTAINS (COMPOSITION d OR VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c)",
            "EHR e CONTAINS VERSION v[LATEST_VERSION]",
        ] {
            let error = CheckedQuery::compositions(&over(from)).expect_err(from);
            assert!(
                matches!(error, AqlError::Unscoped { .. }),
                "{from}: {error:?}"
            );
        }
    }

    /// Returns the composition query over `EHR e CONTAINS VERSION
    /// v[LATEST_VERSION] CONTAINS COMPOSITION c`, ordered by `order`.
    fn ordered(order: &str) -> String {
        format!(
            "SELECT e/ehr_id/value AS ehr_id, v/uid/value AS version_uid, c AS composition \
             FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c ORDER BY {order}"
        )
    }

    #[test]
    fn an_order_ending_in_the_version_uid_checks_in_either_direction() {
        for order in [
            "v/uid/value",
            "v/commit_audit/time_committed/value, v/uid/value",
            "v/commit_audit/time_committed/value DESC, v/uid/value DESC",
            "e/ehr_id/value, v/uid/value ASC",
        ] {
            CheckedQuery::compositions(&ordered(order)).expect(order);
        }
    }

    // NOTE: openEHR RM Common 1.1.0 §6 (Contributions): the versions of one
    // contribution share its commit time, so each of these orders can tie.
    #[test]
    fn an_order_not_ending_in_the_version_uid_is_refused() {
        for order in [
            "v/commit_audit/time_committed/value",
            "v/uid/value, v/commit_audit/time_committed/value",
            "c/context/start_time/value",
            "e/ehr_id/value",
            "c/uid/value",
            "v/uid",
            "v/uid/value/x",
        ] {
            let error = CheckedQuery::compositions(&ordered(order)).expect_err(order);
            assert!(
                matches!(error, AqlError::Unkeyed { .. }),
                "{order}: {error:?}"
            );
        }
    }

    #[test]
    fn the_uid_of_a_version_outside_the_latest_scope_is_no_key() {
        let error = CheckedQuery::compositions(
            "SELECT e/ehr_id/value AS ehr_id, w/uid/value AS version_uid, c AS composition \
             FROM EHR e CONTAINS (VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
             AND VERSION w[ALL_VERSIONS] CONTAINS OBSERVATION o) ORDER BY w/uid/value",
        )
        .expect_err("w reads every version");
        assert!(matches!(error, AqlError::Unkeyed { .. }), "{error:?}");
    }

    #[test]
    fn a_visit_query_below_no_latest_version_scope_is_refused() {
        let error = CheckedQuery::visits(
            "SELECT e/ehr_id/value AS ehr_id, c/name/value AS visit_source, \
             c/context/start_time/value AS visit_start, c/context/end_time/value AS visit_end \
             FROM EHR e CONTAINS COMPOSITION c ORDER BY c/context/start_time/value, v/uid/value",
        )
        .expect_err("the visit query is unscoped");
        assert!(matches!(error, AqlError::Unscoped { .. }), "{error:?}");
    }

    /// A visit query with the four projections below a `LATEST_VERSION`
    /// scope, with `filter` as its `WHERE` clause.
    fn visits_where(filter: &str) -> String {
        format!(
            "SELECT e/ehr_id/value AS ehr_id, c/name/value AS visit_source, \
             c/context/start_time/value AS visit_start, c/context/end_time/value AS visit_end \
             FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
             {filter} ORDER BY c/context/start_time/value, v/uid/value"
        )
    }

    #[test]
    fn a_visit_query_naming_no_since_checks() {
        let query = CheckedQuery::visits(&visits_where(
            "WHERE c/context/start_time/value >= '2026-01-01T00:00:00Z'",
        ))
        .expect("the visit query checks");
        assert_eq!(Some(2), query.column("visit_start"));
        assert!(!query.names_parameter(SINCE_PARAMETER));
    }

    #[test]
    fn a_visit_query_naming_since_is_refused() {
        let error = CheckedQuery::visits(&visits_where(
            "WHERE v/commit_audit/time_committed/value > $since",
        ))
        .expect_err("the visit query names $since");
        assert!(
            matches!(
                error,
                AqlError::Windowed {
                    query: "visit query"
                }
            ),
            "{error:?}"
        );
        assert!(error.to_string().contains("$since"), "{error}");
    }

    #[test]
    fn a_composition_query_naming_since_checks() {
        let query = CheckedQuery::compositions(&over(
            "EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
             WHERE v/commit_audit/time_committed/value > $since",
        ))
        .expect("the composition query binds $since");
        assert!(query.names_parameter(SINCE_PARAMETER));
    }

    #[test]
    fn a_composition_below_a_latest_version_scope_checks() {
        for from in [
            "EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c",
            "EHR e CONTAINS VERSIONED_OBJECT vo CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c",
            "EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
             CONTAINS OBSERVATION o[openEHR-EHR-OBSERVATION.laboratory_test_result.v1]",
            "EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
             NOT CONTAINS COMPOSITION d",
        ] {
            CheckedQuery::compositions(&over(from)).expect(from);
        }
    }
}
