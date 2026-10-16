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
use openehr_query::ast::ColumnExpr;
use openehr_query::parser;
use openehr_query::parser::ParseError;

use crate::facade::search::declaration::Node;

/// The aliases the facade's `FROM` binds: the EHR, the version and the
/// composition.
pub const ALIASES: [&str; 3] = ["e", "v", "c"];

/// The alias the first bound node takes, as a path is checked at load.
pub const NODE_ALIAS: &str = "n0";

/// The `FROM` every search query reads, before any bound node.
///
/// `LATEST_VERSION` is the AQL version predicate for the newest version of
/// each versioned object (QUERY AQL, the `versionPredicate` rule), which is
/// the version a read answers.
const FROM: &str = "EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c";

/// The columns a row query selects, in the order a row is read.
const COLUMNS: &str = "e/ehr_id/value AS ehr_id, v/uid/value AS version_uid, c AS composition";

/// The path the template condition compares.
///
/// `LOCATABLE.archetype_details` carries the `template_id` a root archetype was
/// built against (openEHR RM Common, `ARCHETYPED`).
const TEMPLATE_PATH: &str = "c/archetype_details/template_id/value";

/// The parameter the template id is bound to.
const TEMPLATE_PARAMETER: &str = "template";

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

/// The `WHERE` conditions of one query, the nodes it binds, and the
/// parameters they bind.
#[derive(Debug, Clone, PartialEq)]
pub struct Conditions {
    /// The conditions, each joined to the rest with `AND`.
    clauses: Vec<String>,
    /// The bound values, by parameter name.
    parameters: QueryParameters,
    /// The number the next bound parameter is named with.
    next: usize,
    /// The nodes the `FROM` binds below the composition, in alias order.
    nodes: Vec<Node>,
}

impl Conditions {
    /// Returns the conditions of a search over the template `template_id`.
    #[must_use]
    pub fn new(template_id: &str) -> Self {
        let mut parameters = QueryParameters::new();
        parameters.insert(
            String::from(TEMPLATE_PARAMETER),
            serde_json::Value::String(template_id.to_owned()),
        );
        Self {
            clauses: vec![format!("{TEMPLATE_PATH} = ${TEMPLATE_PARAMETER}")],
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

    /// Whether the `FROM` binds a node, so one composition may answer more
    /// than one row.
    #[must_use]
    pub fn binds_nodes(&self) -> bool {
        !self.nodes.is_empty()
    }

    /// Returns the number of nodes the `FROM` binds, each one more column of
    /// a row.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Adds `clause`, which every row must also satisfy.
    pub fn push(&mut self, clause: String) {
        self.clauses.push(clause);
    }

    /// Returns the `FROM` expression with every bound node.
    ///
    /// Two bound nodes are two class expressions joined with `AND` under the
    /// composition (QUERY AQL, the `containsExpr` rule). The archetype id is
    /// one the template holds as the node's root, checked when the
    /// declaration loaded, so it is written as the class predicate.
    fn from(&self) -> String {
        let bound: Vec<String> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| format!("{} n{index}[{}]", node.class, node.archetype))
            .collect();
        match bound.as_slice() {
            [] => String::from(FROM),
            [one] => format!("{FROM} CONTAINS {one}"),
            many => format!("{FROM} CONTAINS ({})", many.join(" AND ")),
        }
    }

    /// Returns the `WHERE` expression, each condition parenthesised.
    fn text(&self) -> String {
        self.clauses
            .iter()
            .map(|clause| format!("({clause})"))
            .collect::<Vec<String>>()
            .join(" AND ")
    }
}

/// One `ORDER BY` key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderKey {
    /// The path ordered by.
    pub path: String,
    /// Whether the key orders downward.
    pub descending: bool,
}

/// Returns the row query `conditions` and `order` describe.
#[must_use]
pub fn rows_text(conditions: &Conditions, order: &[OrderKey]) -> String {
    let keys = order
        .iter()
        .map(|key| {
            format!(
                "{} {}",
                key.path,
                if key.descending { "DESC" } else { "ASC" }
            )
        })
        .collect::<Vec<String>>()
        .join(", ");
    let nodes: String = (0..conditions.node_count())
        .map(|index| format!(", n{index} AS n{index}"))
        .collect::<Vec<String>>()
        .concat();
    format!(
        "SELECT {COLUMNS}{nodes} FROM {} WHERE {} ORDER BY {keys}",
        conditions.from(),
        conditions.text()
    )
}

/// Returns the count query `conditions` describe.
///
/// `COUNT(*)` counts every row and `COUNT(DISTINCT expression)` every
/// distinct value (QUERY AQL §3.9.1.1). With no bound node a row is one
/// composition; with one, the count is of distinct version uids, so either
/// way it is the number of matching compositions.
#[must_use]
pub fn count_text(conditions: &Conditions) -> String {
    let counted = if conditions.binds_nodes() {
        "COUNT(DISTINCT v/uid/value)"
    } else {
        "COUNT(*)"
    };
    format!(
        "SELECT {counted} AS total FROM {} WHERE {}",
        conditions.from(),
        conditions.text()
    )
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

/// Returns the request body of the count query.
#[must_use]
pub fn count_request(conditions: &Conditions) -> AdhocQueryExecute {
    AdhocQueryExecute {
        q: count_text(conditions),
        offset: None,
        fetch: None,
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

/// Checks that `path` is one AQL identified path rooted at one of
/// [`ALIASES`] or at [`NODE_ALIAS`], and returns why when it is not.
///
/// The path is parsed as the one column of a query over the facade's `FROM`,
/// so anything that is not a single path (a second column, a literal, a
/// function, a trailing clause) is refused before it can reach a query.
///
/// # Errors
///
/// Returns the reason the path is refused.
pub fn check_path(path: &str) -> Result<(), String> {
    let query = parser::parse_str(&format!(
        "SELECT {path} FROM {FROM} CONTAINS CLUSTER {NODE_ALIAS}[openEHR-EHR-CLUSTER.probe.v1]"
    ))
    .map_err(|error| format!("it does not parse as an AQL path: {error}"))?;
    let [column] = query.select.columns.as_slice() else {
        return Err(String::from("it is more than one AQL path"));
    };
    if column.alias.is_some() {
        return Err(String::from("it carries an alias"));
    }
    match column.column {
        ColumnExpr::Path(ref identified)
            if ALIASES.contains(&identified.root.as_str()) || identified.root == NODE_ALIAS =>
        {
            Ok(())
        }
        ColumnExpr::Path(ref identified) => Err(format!(
            "it is rooted at `{}`; a search path is rooted at e (the EHR), v (the version), c (the composition) or the bound node",
            identified.root
        )),
        _ => Err(String::from("it is no AQL identified path")),
    }
}

#[cfg(test)]
mod tests {
    use super::{Conditions, DEFAULT_ORDER, OrderKey, check_path, count_text, parses, rows_text};

    fn default_order() -> Vec<OrderKey> {
        DEFAULT_ORDER
            .iter()
            .map(|path| OrderKey {
                path: (*path).to_owned(),
                descending: false,
            })
            .collect()
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
        conditions.push(format!("c/content/code_string = {code}"));
        let system = conditions.bind("SNOMED-CT");
        conditions.push(format!("c/content/terminology_id/value = {system}"));
        conditions.push(String::from("NOT EXISTS c/content/terminology_id/value"));
        let pattern = conditions.bind("diab*");
        conditions.push(format!("c/content/value LIKE {pattern}"));
        let low = conditions.bind("2026-01-01");
        let high = conditions.bind("2026-02-01");
        conditions.push(format!(
            "NOT (c/context/start_time/value >= {low} AND c/context/start_time/value < {high})"
        ));
        let ehr = conditions.bind("bd6b1e5a-3b9b-4a4a-9e0b-9f4b3a0c9f11");
        let other = conditions.bind("aaaaaaaa-3b9b-4a4a-9e0b-9f4b3a0c9f11");
        conditions.push(format!(
            "e/ehr_id/value = {ehr} OR e/ehr_id/value = {other}"
        ));
        let container = conditions.bind("8849182c-82ad-4088-a07f-48ead4180515::*");
        conditions.push(format!(
            "e/ehr_id/value = {ehr} AND v/uid/value LIKE {container}"
        ));
        let mut order = vec![OrderKey {
            path: String::from("c/context/start_time/value"),
            descending: true,
        }];
        order.extend(default_order());
        parses(&rows_text(&conditions, &order)).expect("the row query parses");
        parses(&count_text(&conditions)).expect("the count query parses");
    }

    #[test]
    fn a_bound_node_reads_code_and_system_from_one_node_and_counts_compositions() {
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
        conditions.push(format!(
            "{first}/data[at0001]/items[at0002]/value/defining_code/terminology_id/value = {system} AND {first}/data[at0001]/items[at0002]/value/defining_code/code_string = {code}"
        ));
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
        let count = count_text(&conditions);
        assert!(count.contains("COUNT(DISTINCT v/uid/value)"), "{count}");
        parses(&count).expect("the count query parses");

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
        assert!(count_text(&Conditions::new("t")).contains("COUNT(*)"));
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
