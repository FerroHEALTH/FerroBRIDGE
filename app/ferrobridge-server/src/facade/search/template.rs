// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Placing a declared parameter in the context's template.
//!
//! AQL 1.1.0 defines no answer to which node a path through a multi-valued
//! attribute yields, and two `WHERE` conditions on two such paths need not
//! read the same node (<https://specifications.openehr.org/releases/QUERY/Release-1.1.0/AQL.html>). So the paths of one
//! parameter are read under one node the query binds in its `FROM`, and no
//! node between that one and the compared value may occur more than once:
//! below the bound node each path then has one value, and a token's code and
//! system come from one coded value, as R4 matches `[system]|[code]` against
//! one `Coding` (<https://hl7.org/fhir/R4/search.html#token>). The Web
//! Template's occurrences decide it (Simplified Formats, §Web Template
//! Metadata). No specification governs the placement: our own design.

use openehr_mapping_core::index::ResolvedNode;
use openehr_mapping_core::index::WebTemplateIndex;

use crate::facade::search::declaration::Coded;
use crate::facade::search::declaration::Node;
use crate::facade::search::declaration::ParameterType;

/// Places `path`, read under `node` (or under the composition), and returns
/// the coded value's type for a `token`.
///
/// Every placement of the node the template holds is checked, and all of them
/// must read the path the same way.
///
/// # Errors
///
/// Returns why the path cannot be placed: the template holds no such node or
/// no node at the path, a node below the bound one may occur more than once,
/// a `token` names no coded value, or a `string` or `date` names no data
/// value.
pub fn locate(
    index: &WebTemplateIndex,
    node: Option<&Node>,
    path: &str,
    kind: ParameterType,
) -> Result<Option<Coded>, String> {
    let roots: Vec<&ResolvedNode> = match node {
        None => vec![index.root()],
        Some(wanted) => index
            .nodes()
            .filter(|candidate| {
                candidate.node_id() == Some(wanted.archetype.as_str())
                    && candidate.rm_type() == wanted.class
            })
            .collect(),
    };
    if roots.is_empty() {
        let wanted = node.map_or_else(String::new, |node| {
            format!("{} {}", node.class, node.archetype)
        });
        return Err(format!("the template holds no {wanted}"));
    }
    let mut answer: Option<Option<Coded>> = None;
    for root in roots {
        let placed = place(index, root, path, kind)?;
        if answer.is_some_and(|earlier| earlier != placed) {
            return Err(String::from(
                "the node's placements in the template read the path as different types",
            ));
        }
        answer = Some(placed);
    }
    answer.ok_or_else(|| String::from("the template holds no such node"))
}

/// Places `path` under one placement `root` of the bound node.
fn place(
    index: &WebTemplateIndex,
    root: &ResolvedNode,
    path: &str,
    kind: ParameterType,
) -> Result<Option<Coded>, String> {
    let base = root.aql_path().as_str();
    let target = format!("{base}/{path}");
    let mut deepest: Option<&ResolvedNode> = None;
    for candidate in index.nodes() {
        let candidate_path = candidate.aql_path().as_str();
        if candidate_path.len() <= base.len() || !leads_to(candidate_path, &target) {
            continue;
        }
        if candidate.repeats() {
            return Err(format!(
                "{candidate_path} may occur more than once below the bound node, so its values cannot be read as one; bind the repeated node with `node`"
            ));
        }
        if deepest.is_none_or(|found| found.aql_path().as_str().len() < candidate_path.len()) {
            deepest = Some(candidate);
        }
    }
    let value = deepest.ok_or_else(|| format!("the template holds no node at {target}"))?;
    match kind {
        ParameterType::Token => {
            if value.aql_path().as_str() != target {
                return Err(format!(
                    "a token names the coded value itself, and {target} lies below {}",
                    value.aql_path().as_str()
                ));
            }
            match value.rm_type() {
                // NOTE: openEHR RM Data Types: DV_CODED_TEXT is a DV_TEXT, so a
                // DV_TEXT node may carry the coded text a token compares.
                "DV_CODED_TEXT" | "DV_TEXT" => Ok(Some(Coded::Text)),
                "CODE_PHRASE" => Ok(Some(Coded::Phrase)),
                other => Err(format!(
                    "a token compares a coded text or a code phrase, and {target} is a {other}"
                )),
            }
        }
        ParameterType::String | ParameterType::Date => {
            if value.rm_type().starts_with("DV_") {
                Ok(None)
            } else {
                Err(format!(
                    "{target} reads no data value: the nearest node is a {}",
                    value.rm_type()
                ))
            }
        }
        ParameterType::Reference => Err(String::from(
            "a reference parameter is the patient section's",
        )),
    }
}

/// Whether `node` is `target` or one of its ancestors, at a segment boundary.
fn leads_to(node: &str, target: &str) -> bool {
    target
        .strip_prefix(node)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::leads_to;

    #[test]
    fn a_prefix_counts_only_at_a_segment_boundary() {
        assert!(leads_to("/a/items[at0002]", "/a/items[at0002]/value"));
        assert!(leads_to("/a/items[at0002]", "/a/items[at0002]"));
        assert!(!leads_to("/a/items[at0002]", "/a/items[at00021]/value"));
    }
}
