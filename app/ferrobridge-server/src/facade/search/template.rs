// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Placing a declared parameter, and the context's entry, in the context's
//! template.
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

/// Where the template places the entries of a context's archetype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The RM class and the archetype id of the entry.
    pub entry: Node,
    /// Whether every placement is a `content` item of the composition itself.
    pub top_level: bool,
}

/// Returns where the template places the nodes carrying `archetype`, or
/// `None` when it holds none or gives them two RM classes.
///
/// An AQL `CONTAINS` finds a node at any depth (QUERY AQL 1.1.0 §3.13.2.2
/// defines no direct-child form), while the search answers the occurrences
/// the template's repeating nodes name. The two agree for certain only when
/// every placement is a `content` item of the composition itself, so only
/// then is the count one `CONTAINS` query (no specification governs the
/// count: our own design).
#[must_use]
pub fn placement(index: &WebTemplateIndex, archetype: &str) -> Option<Placement> {
    let placed: Vec<&ResolvedNode> = index
        .nodes()
        .filter(|node| node.node_id() == Some(archetype))
        .collect();
    let first = placed.first()?;
    if placed.iter().any(|node| node.rm_type() != first.rm_type()) {
        return None;
    }
    let top = index.root().children();
    Some(Placement {
        entry: Node {
            class: first.rm_type().to_owned(),
            archetype: archetype.to_owned(),
        },
        top_level: placed.iter().all(|node| top.contains(node.flat_id())),
    })
}

/// Whether `node` is `target` or one of its ancestors, at a segment boundary.
fn leads_to(node: &str, target: &str) -> bool {
    target
        .strip_prefix(node)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use openehr_mapping_core::index::WebTemplateIndex;
    use openehr_mapping_core::template::Generation;
    use openehr_mapping_core::template::TemplateSource;
    use openehr_mapping_core::template::web_template;
    use openehr_sdt::flat::webtemplate::model::WebTemplate;
    use openehr_sdt::flat::webtemplate::model::WebTemplateNode;

    use super::leads_to;
    use super::placement;

    /// The archetype of the diagnosis entry the fixture template holds.
    const DIAGNOSIS: &str = "openEHR-EHR-EVALUATION.problem_diagnosis.v1";

    /// Returns the Web Template of the fixture diagnosis template.
    fn diagnose() -> WebTemplate {
        let opt = openehr_its::opt14::from_xml(ferrobridge_testkit::fixtures::DIAGNOSE_OPT)
            .expect("the fixture template reads");
        web_template(&TemplateSource::Opt14(Box::new(opt))).expect("the fixture template builds")
    }

    /// Moves the diagnosis entry of `built` into a synthetic `SECTION`, and
    /// with `keep` leaves a copy at the top level too.
    fn sectioned(mut built: WebTemplate, keep: bool) -> WebTemplate {
        let at = built
            .tree
            .children
            .iter()
            .position(|child| child.node_id.as_deref() == Some(DIAGNOSIS))
            .expect("the template holds the diagnosis at the top level");
        let entry = if keep {
            built
                .tree
                .children
                .get(at)
                .cloned()
                .expect("the position holds")
        } else {
            built.tree.children.remove(at)
        };
        let mut section: WebTemplateNode = entry.clone();
        section.id = String::from("synthetic_section");
        section.rm_type = String::from("SECTION");
        section.node_id = Some(String::from("openEHR-EHR-SECTION.adhoc.v1"));
        section.aql_path = String::from("/content[openEHR-EHR-SECTION.adhoc.v1]");
        section.children = vec![entry];
        built.tree.children.push(section);
        built
    }

    #[test]
    fn a_top_level_entry_is_counted_over_contains() {
        let index = WebTemplateIndex::over(diagnose(), Generation::Adl14).expect("it indexes");
        let found = placement(&index, DIAGNOSIS).expect("the template places the entry");
        assert_eq!("EVALUATION", found.entry.class);
        assert_eq!(DIAGNOSIS, found.entry.archetype);
        assert!(found.top_level);
    }

    #[test]
    fn an_entry_nested_in_a_section_is_not_top_level() {
        for keep in [false, true] {
            let index = WebTemplateIndex::over(sectioned(diagnose(), keep), Generation::Adl14)
                .expect("it indexes");
            let found = placement(&index, DIAGNOSIS).expect("the template places the entry");
            assert_eq!("EVALUATION", found.entry.class);
            assert!(!found.top_level, "keep={keep}");
        }
    }

    #[test]
    fn an_archetype_the_template_lacks_has_no_placement() {
        let index = WebTemplateIndex::over(diagnose(), Generation::Adl14).expect("it indexes");
        assert_eq!(None, placement(&index, "openEHR-EHR-EVALUATION.absent.v1"));
    }

    #[test]
    fn a_prefix_counts_only_at_a_segment_boundary() {
        assert!(leads_to("/a/items[at0002]", "/a/items[at0002]/value"));
        assert!(leads_to("/a/items[at0002]", "/a/items[at0002]"));
        assert!(!leads_to("/a/items[at0002]", "/a/items[at00021]/value"));
    }
}
