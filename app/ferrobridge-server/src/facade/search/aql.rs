// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The AQL a search runs, built from a declaration and checked at load.
//!
//! The facade writes the `SELECT`, the `FROM` and the template condition
//! itself: the latest version of every composition of the context's template.
//! A parameter read under a repeated node binds that node in the `FROM`, one
//! alias per parameter occurrence, so the conditions of one occurrence read
//! one node, and two occurrences (an `AND`) may each match a node of their
//! own. A row is then one composition with one node per bound alias, and it
//! selects each bound node, so the facade answers the entry that holds the
//! nodes that matched (no specification governs how a row names its entry:
//! our own design). A declaration contributes paths only, each compared
//! against a bound `$` parameter and never spliced in as a value (QUERY AQL
//! §3.5). The paging is the `offset` and `fetch` of ITS-REST 1.1.0
//! (`query-codegen.openapi.yaml`, `components.parameters.offset`), which name
//! row numbers, so every query carries the declaration's total order. No
//! specification governs the shape: our own design.

use openehr_its::rest::generated::query::AdhocQueryExecute;
use openehr_its::rest::generated::query::QueryParameters;
use openehr_query::ast::AggregateCall;
use openehr_query::ast::ArchetypePredicate;
use openehr_query::ast::ClassExprOperand;
use openehr_query::ast::ColumnExpr;
use openehr_query::ast::CompareOperand;
use openehr_query::ast::ContainsConstraint;
use openehr_query::ast::ContainsExpr;
use openehr_query::ast::IdentifiedExpr;
use openehr_query::ast::IdentifiedPath;
use openehr_query::ast::LikeOperand;
use openehr_query::ast::ObjectPath;
use openehr_query::ast::OrderByExpr;
use openehr_query::ast::PathPart;
use openehr_query::ast::PathPredicate;
use openehr_query::ast::SelectClause;
use openehr_query::ast::SelectExpr;
use openehr_query::ast::SelectQuery;
use openehr_query::ast::SortOrder;
use openehr_query::ast::Terminal;
use openehr_query::ast::VersionPredicate;
use openehr_query::ast::WhereExpr;
use openehr_query::lexer::CompOp;
use openehr_query::parser;
use openehr_query::parser::ParseError;
use openehr_query::printer;

use crate::facade::search::declaration::Node;

/// The aliases the facade's `FROM` binds: the EHR, the version and the
/// composition.
pub const ALIASES: [&str; 3] = ["e", "v", "c"];

/// The alias the first bound node takes, as a path is checked at load.
pub const NODE_ALIAS: &str = "n0";

/// The `FROM` a path is checked under, the facade's own with one bound node.
///
/// `LATEST_VERSION` is the AQL version predicate for the newest version of
/// each versioned object (QUERY AQL, the `versionPredicate` rule), which is
/// the version a read answers.
const PROBE_FROM: &str = "EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
    CONTAINS CLUSTER n0[openEHR-EHR-CLUSTER.probe.v1]";

/// The parameter the template id is bound to.
const TEMPLATE_PARAMETER: &str = "$template";

/// The position of each column in a row.
pub mod column {
    /// The `ehr_id` of the EHR.
    pub const EHR_ID: usize = 0;
    /// The `OBJECT_VERSION_ID` of the version.
    pub const VERSION_UID: usize = 1;
    /// The composition, canonical JSON.
    pub const COMPOSITION: usize = 2;
    /// The first bound node, canonical JSON; node `n<i>` is column
    /// `FIRST_NODE + i`.
    pub const FIRST_NODE: usize = 3;
}

/// The order a declaration with no `order` key pages in.
///
/// The commit time first and the version uid second, so two commits in one
/// instant still have a fixed order and the rows of one composition sit
/// together (no specification governs the default: our own design).
pub const DEFAULT_ORDER: [&str; 2] = ["v/commit_audit/time_committed/value", "v/uid/value"];

/// Returns the path below `root` that steps through `attributes`.
#[must_use]
pub fn attribute_path(root: &str, attributes: &[&str]) -> IdentifiedPath {
    let parts = attributes
        .iter()
        .map(|&name| PathPart {
            name: name.to_owned(),
            predicate: None,
        })
        .collect();
    IdentifiedPath::new(root.to_owned(), None, Some(ObjectPath { parts }))
}

/// Returns `path` read below `alias`, then through `attributes`.
#[must_use]
pub fn below(alias: &str, path: &ObjectPath, attributes: &[&str]) -> IdentifiedPath {
    let mut extended = path.clone();
    extended
        .parts
        .extend(attributes.iter().map(|&name| PathPart {
            name: name.to_owned(),
            predicate: None,
        }));
    IdentifiedPath::new(alias.to_owned(), None, Some(extended))
}

/// Returns the condition that `path` compares to the bound `parameter` by
/// `op`.
#[must_use]
pub fn compare(path: IdentifiedPath, op: CompOp, parameter: &str) -> WhereExpr {
    WhereExpr::identified(IdentifiedExpr::Compare {
        lhs: CompareOperand::Path(path),
        op,
        rhs: Terminal::Parameter(parameter.to_owned()),
    })
}

/// Returns the condition that `path` matches the pattern bound to
/// `parameter` (QUERY AQL §3.8.1.1).
#[must_use]
pub fn like(path: IdentifiedPath, parameter: &str) -> WhereExpr {
    WhereExpr::identified(IdentifiedExpr::Like {
        path,
        operand: LikeOperand::Parameter(parameter.to_owned()),
    })
}

/// Returns the condition that `path` holds a value.
#[must_use]
pub fn exists(path: IdentifiedPath) -> WhereExpr {
    WhereExpr::identified(IdentifiedExpr::Exists(path))
}

/// Returns the negation of `condition`.
#[must_use]
pub fn not(condition: WhereExpr) -> WhereExpr {
    WhereExpr::Not(Box::new(condition))
}

/// Returns the conjunction of `conditions`, `None` for none.
#[must_use]
pub fn all(conditions: impl IntoIterator<Item = WhereExpr>) -> Option<WhereExpr> {
    conditions
        .into_iter()
        .reduce(|left, right| WhereExpr::And(Box::new(left), Box::new(right)))
}

/// Returns the disjunction of `conditions`, `None` for none.
#[must_use]
pub fn any(conditions: impl IntoIterator<Item = WhereExpr>) -> Option<WhereExpr> {
    conditions
        .into_iter()
        .reduce(|left, right| WhereExpr::Or(Box::new(left), Box::new(right)))
}

/// The `WHERE` conditions of one query, the nodes it binds, and the
/// parameters they bind.
#[derive(Debug, Clone, PartialEq)]
pub struct Conditions {
    /// The conditions, each joined to the rest with `AND`.
    clauses: Vec<WhereExpr>,
    /// The bound values, by parameter name.
    parameters: QueryParameters,
    /// The number the next bound parameter is named with.
    next: usize,
    /// The nodes the `FROM` binds below the composition, in alias order.
    nodes: Vec<Node>,
}

impl Conditions {
    /// Returns the conditions of a search over the template `template_id`.
    ///
    /// `LOCATABLE.archetype_details` carries the `template_id` a root
    /// archetype was built against (openEHR RM Common, `ARCHETYPED`).
    #[must_use]
    pub fn new(template_id: &str) -> Self {
        let mut parameters = QueryParameters::new();
        parameters.insert(
            String::from(TEMPLATE_PARAMETER.trim_start_matches('$')),
            serde_json::Value::String(template_id.to_owned()),
        );
        let template = attribute_path("c", &["archetype_details", "template_id", "value"]);
        Self {
            clauses: vec![compare(template, CompOp::Eq, TEMPLATE_PARAMETER)],
            parameters,
            next: 0,
            nodes: Vec::new(),
        }
    }

    /// Binds `value` to a fresh parameter and returns its `$name`.
    pub fn bind(&mut self, value: impl Into<String>) -> String {
        let name = format!("p{}", self.next);
        self.next = self.next.saturating_add(1);
        self.parameters
            .insert(name.clone(), serde_json::Value::String(value.into()));
        format!("${name}")
    }

    /// Binds `node` under the composition with a fresh alias and returns the
    /// alias, or returns `c` when the paths are read under the composition.
    pub fn alias(&mut self, node: Option<&Node>) -> String {
        let Some(node) = node else {
            return String::from("c");
        };
        let alias = format!("n{}", self.nodes.len());
        self.nodes.push(node.clone());
        alias
    }

    /// Returns the number of nodes the `FROM` binds, each one more column of
    /// a row.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Adds `clause`, which every row must also satisfy.
    pub fn push(&mut self, clause: WhereExpr) {
        self.clauses.push(clause);
    }

    /// Returns the `FROM` expression with every bound node.
    ///
    /// Two bound nodes are two class expressions joined with `AND` under the
    /// composition (QUERY AQL, the `containsExpr` rule). The archetype id is
    /// one the template holds as the node's root, checked when the
    /// declaration loaded, so it is written as the class predicate.
    fn from(&self) -> ContainsExpr {
        let bound = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| class(node, &format!("n{index}")))
            .reduce(|left, right| ContainsExpr::And(Box::new(left), Box::new(right)));
        facade_from(bound)
    }

    /// Returns the `WHERE` expression.
    fn condition(&self) -> Option<WhereExpr> {
        all(self.clauses.iter().cloned())
    }
}

/// Returns the class expression that binds `node` to `alias`.
fn class(node: &Node, alias: &str) -> ContainsExpr {
    ContainsExpr::Contained {
        operand: ClassExprOperand::Class {
            rm_type: node.class.clone(),
            variable: Some(alias.to_owned()),
            predicate: Some(PathPredicate::Archetype(ArchetypePredicate::Hrid(
                node.archetype.clone(),
            ))),
        },
        contains: None,
    }
}

/// Returns `operand` constrained to contain `inner`, when there is one.
fn containing(operand: ClassExprOperand, inner: Option<ContainsExpr>) -> ContainsExpr {
    ContainsExpr::Contained {
        operand,
        contains: inner.map(|expr| {
            Box::new(ContainsConstraint {
                negated: false,
                expr,
            })
        }),
    }
}

/// Returns the `FROM` every search query reads, the latest version of every
/// composition of every EHR, with `inner` contained in the composition.
fn facade_from(inner: Option<ContainsExpr>) -> ContainsExpr {
    let composition = containing(
        ClassExprOperand::Class {
            rm_type: String::from("COMPOSITION"),
            variable: Some(String::from("c")),
            predicate: None,
        },
        inner,
    );
    let version = containing(
        ClassExprOperand::Version {
            variable: Some(String::from("v")),
            predicate: Some(VersionPredicate::Latest),
        },
        Some(composition),
    );
    containing(
        ClassExprOperand::Class {
            rm_type: String::from("EHR"),
            variable: Some(String::from("e")),
            predicate: None,
        },
        Some(version),
    )
}

/// Returns one selected column, `column` named `alias`.
fn column(column: ColumnExpr, alias: &str) -> SelectExpr {
    SelectExpr {
        column,
        alias: Some(alias.to_owned()),
    }
}

/// One `ORDER BY` key.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderKey {
    /// The path ordered by.
    pub path: IdentifiedPath,
    /// Whether the key orders downward.
    pub descending: bool,
}

/// Returns the row query `conditions` and `order` describe.
#[must_use]
pub fn rows_query(conditions: &Conditions, order: &[OrderKey]) -> SelectQuery {
    let mut columns = vec![
        column(
            ColumnExpr::Path(attribute_path("e", &["ehr_id", "value"])),
            "ehr_id",
        ),
        column(
            ColumnExpr::Path(attribute_path("v", &["uid", "value"])),
            "version_uid",
        ),
        column(
            ColumnExpr::Path(IdentifiedPath::new(String::from("c"), None, None)),
            "composition",
        ),
    ];
    columns.extend((0..conditions.node_count()).map(|index| {
        let alias = format!("n{index}");
        column(
            ColumnExpr::Path(IdentifiedPath::new(alias.clone(), None, None)),
            &alias,
        )
    }));
    SelectQuery {
        select: SelectClause {
            distinct: false,
            top: None,
            columns,
        },
        from: conditions.from(),
        where_: conditions.condition(),
        order_by: order
            .iter()
            .map(|key| OrderByExpr {
                path: key.path.clone(),
                order: Some(if key.descending {
                    SortOrder::Descending
                } else {
                    SortOrder::Ascending
                }),
            })
            .collect(),
        limit: None,
    }
}

/// Returns the text of the row query `conditions` and `order` describe.
#[must_use]
pub fn rows_text(conditions: &Conditions, order: &[OrderKey]) -> String {
    printer::to_aql(&rows_query(conditions, order), None)
}

/// The alias a count binds the context's entry to.
const ENTRY_ALIAS: &str = "x";

/// Returns the count query `conditions` describe, `entry` the context's
/// entry.
///
/// A query that binds no node counts the entries of `entry` the matching
/// compositions contain, one row per entry under `CONTAINS` (QUERY AQL
/// §3.13.2.2), with `COUNT(*)` (§3.9.1.1). A query that binds a node, or that
/// has no `entry`, counts the rows the row query reads. Either count reads
/// no composition.
#[must_use]
pub fn count_query(conditions: &Conditions, entry: Option<&Node>) -> SelectQuery {
    let from = match entry {
        Some(entry) if conditions.nodes.is_empty() => facade_from(Some(class(entry, ENTRY_ALIAS))),
        _ => conditions.from(),
    };
    SelectQuery {
        select: SelectClause {
            distinct: false,
            top: None,
            columns: vec![column(
                ColumnExpr::Aggregate(AggregateCall::Count {
                    distinct: false,
                    path: None,
                }),
                "total",
            )],
        },
        from,
        where_: conditions.condition(),
        order_by: Vec::new(),
        limit: None,
    }
}

/// Returns the text of the count query `conditions` describe.
#[must_use]
pub fn count_text(conditions: &Conditions, entry: Option<&Node>) -> String {
    printer::to_aql(&count_query(conditions, entry), None)
}

/// Returns the request body of the count query.
#[must_use]
pub fn count_request(conditions: &Conditions, entry: Option<&Node>) -> AdhocQueryExecute {
    AdhocQueryExecute {
        q: count_text(conditions, entry),
        offset: None,
        fetch: None,
        query_parameters: Some(conditions.parameters.clone()),
        additional_properties: std::collections::BTreeMap::new(),
    }
}

/// Returns the request body of the row query, one page of it.
#[must_use]
pub fn rows_request(
    conditions: &Conditions,
    order: &[OrderKey],
    offset: i64,
    fetch: i64,
) -> AdhocQueryExecute {
    AdhocQueryExecute {
        q: rows_text(conditions, order),
        offset: Some(offset),
        fetch: Some(fetch),
        query_parameters: Some(conditions.parameters.clone()),
        additional_properties: std::collections::BTreeMap::new(),
    }
}

/// Checks that `text` parses as AQL.
///
/// # Errors
///
/// Returns the parser's error.
pub fn parses(text: &str) -> Result<(), ParseError> {
    parser::parse_str(text).map(|_query| ())
}

/// Reads `path` as one AQL identified path rooted at one of [`ALIASES`] or
/// at [`NODE_ALIAS`], and returns why when it is not one.
///
/// The path is parsed as the one column of a query over the facade's `FROM`,
/// so anything that is not a single path (a second column, a literal, a
/// function, a trailing clause) is refused before it can reach a query.
///
/// # Errors
///
/// Returns the reason the path is refused.
pub fn check_path(path: &str) -> Result<IdentifiedPath, String> {
    let query = parser::parse_str(&format!("SELECT {path} FROM {PROBE_FROM}"))
        .map_err(|error| format!("it does not parse as an AQL path: {error}"))?;
    let [column] = <[SelectExpr; 1]>::try_from(query.select.columns)
        .map_err(|_columns| String::from("it is more than one AQL path"))?;
    if column.alias.is_some() {
        return Err(String::from("it carries an alias"));
    }
    match column.column {
        ColumnExpr::Path(identified)
            if ALIASES.contains(&identified.root.as_str()) || identified.root == NODE_ALIAS =>
        {
            Ok(identified)
        }
        ColumnExpr::Path(identified) => Err(format!(
            "it is rooted at `{}`; a search path is rooted at e (the EHR), v (the version), c (the composition) or the bound node",
            identified.root
        )),
        _ => Err(String::from("it is no AQL identified path")),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Conditions, DEFAULT_ORDER, OrderKey, all, any, attribute_path, check_path, compare,
        count_text, exists, like, not, parses, rows_text,
    };
    use openehr_query::ast::IdentifiedPath;
    use openehr_query::lexer::CompOp;

    fn default_order() -> Vec<OrderKey> {
        DEFAULT_ORDER
            .iter()
            .map(|path| OrderKey {
                path: check_path(path).expect("a default order path checks"),
                descending: false,
            })
            .collect()
    }

    /// Returns the checked path `text`.
    fn path(text: &str) -> IdentifiedPath {
        check_path(text).expect("the path checks")
    }

    #[test]
    fn the_bare_row_query_parses_and_binds_the_template() {
        let conditions = Conditions::new("ferrobridge.diagnose.v1");
        let text = rows_text(&conditions, &default_order());
        parses(&text).expect("the row query parses");
        assert!(text.contains("$template"), "{text}");
        assert_eq!(
            Some(&serde_json::json!("ferrobridge.diagnose.v1")),
            conditions.parameters.get("template")
        );
    }

    #[test]
    fn every_condition_form_the_criteria_write_parses() {
        let mut conditions = Conditions::new("t");
        let code = conditions.bind("44054006");
        conditions.push(compare(path("c/content/code_string"), CompOp::Eq, &code));
        let system = conditions.bind("SNOMED-CT");
        conditions.push(compare(
            path("c/content/terminology_id/value"),
            CompOp::Eq,
            &system,
        ));
        conditions.push(not(exists(path("c/content/terminology_id/value"))));
        let pattern = conditions.bind("diab*");
        conditions.push(like(path("c/content/value"), &pattern));
        let low = conditions.bind("2026-01-01");
        let high = conditions.bind("2026-02-01");
        let start = path("c/context/start_time/value");
        conditions.push(not(all([
            compare(start.clone(), CompOp::Ge, &low),
            compare(start, CompOp::Lt, &high),
        ])
        .expect("two conditions")));
        let ehr = conditions.bind("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11");
        let other = conditions.bind("aaaaaaaa-3b9b-4a4a-9e0b-9f4b3a0c9f11");
        conditions.push(
            any([
                compare(path("e/ehr_id/value"), CompOp::Eq, &ehr),
                compare(path("e/ehr_id/value"), CompOp::Eq, &other),
            ])
            .expect("two conditions"),
        );
        let container = conditions.bind("8849182c-82ad-4088-a07f-48ead4180515::*");
        conditions.push(
            all([
                compare(path("e/ehr_id/value"), CompOp::Eq, &ehr),
                like(path("v/uid/value"), &container),
            ])
            .expect("two conditions"),
        );
        let mut order = vec![OrderKey {
            path: path("c/context/start_time/value"),
            descending: true,
        }];
        order.extend(default_order());
        parses(&rows_text(&conditions, &order)).expect("the row query parses");
    }

    #[test]
    fn a_bound_node_reads_code_and_system_from_one_node() {
        // #526: the code and the system of one token value are read under one
        // alias the FROM binds, never as two leaves through the repeated node.
        let node = crate::facade::search::declaration::Node {
            class: String::from("EVALUATION"),
            archetype: String::from("openEHR-EHR-EVALUATION.problem_diagnosis.v1"),
        };
        let mut conditions = Conditions::new("t");
        let first = conditions.alias(Some(&node));
        let system = conditions.bind("SNOMED-CT");
        let code = conditions.bind("44054006");
        conditions.push(
            all([
                compare(
                    path(&format!(
                        "{first}/data[at0001]/items[at0002]/value/defining_code/terminology_id/value"
                    )), CompOp::Eq,
                    &system,
                ),
                compare(
                    path(&format!(
                        "{first}/data[at0001]/items[at0002]/value/defining_code/code_string"
                    )), CompOp::Eq,
                    &code,
                ),
            ])
            .expect("two conditions"),
        );
        let rows = rows_text(&conditions, &default_order());
        assert!(
            rows.contains(
                "CONTAINS COMPOSITION c CONTAINS EVALUATION n0[openEHR-EHR-EVALUATION.problem_diagnosis.v1] WHERE"
            ),
            "{rows}"
        );
        assert!(
            rows.contains("c AS composition, n0 AS n0 FROM"),
            "a row names the node that matched: {rows}"
        );
        parses(&rows).expect("the row query parses");

        let second = conditions.alias(Some(&node));
        assert_eq!("n1", second);
        assert_eq!("c", conditions.alias(None));
        let rows = rows_text(&conditions, &default_order());
        assert!(
            rows.contains(
                "CONTAINS (EVALUATION n0[openEHR-EHR-EVALUATION.problem_diagnosis.v1] AND EVALUATION n1[openEHR-EHR-EVALUATION.problem_diagnosis.v1])"
            ),
            "{rows}"
        );
        assert!(rows.contains("n0 AS n0, n1 AS n1 FROM"), "{rows}");
        parses(&rows).expect("two bound nodes parse");
    }

    #[test]
    fn an_entry_count_contains_the_entry_and_reads_no_composition() {
        let entry = crate::facade::search::declaration::Node {
            class: String::from("EVALUATION"),
            archetype: String::from("openEHR-EHR-EVALUATION.problem_diagnosis.v1"),
        };
        let mut conditions = Conditions::new("t");
        let ehr = conditions.bind("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11");
        conditions.push(compare(
            attribute_path("e", &["ehr_id", "value"]),
            CompOp::Eq,
            &ehr,
        ));
        let text = count_text(&conditions, Some(&entry));
        assert!(
            text.starts_with(
                "SELECT COUNT(*) AS total FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c CONTAINS EVALUATION x[openEHR-EHR-EVALUATION.problem_diagnosis.v1] WHERE"
            ),
            "{text}"
        );
        parses(&text).expect("the entry count parses");
        let bare = count_text(&conditions, None);
        assert!(bare.contains("CONTAINS COMPOSITION c WHERE"), "{bare}");
        parses(&bare).expect("the row count parses");

        let node = conditions.alias(Some(&entry));
        conditions.push(exists(attribute_path(&node, &["data"])));
        let bound = count_text(&conditions, Some(&entry));
        assert!(
            bound.contains("CONTAINS COMPOSITION c CONTAINS EVALUATION n0["),
            "a query binding a node counts its rows: {bound}"
        );
        assert!(!bound.contains(" x["), "{bound}");
        parses(&bound).expect("the bound count parses");
    }

    #[test]
    fn a_path_is_one_identified_path_over_the_three_aliases() {
        check_path("c/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0002]/value/value")
            .expect("a composition path checks");
        check_path("v/commit_audit/time_committed/value").expect("a version path checks");
        assert!(
            check_path("x/value").is_err(),
            "an unbound alias is refused"
        );
        assert!(check_path("c/a, c/b").is_err(), "two paths are refused");
        assert!(check_path("'text'").is_err(), "a literal is refused");
        assert!(
            check_path("c/a FROM EHR e").is_err(),
            "a trailing clause is refused"
        );
        assert!(check_path("c/a AS a").is_err(), "an alias is refused");
    }
}
