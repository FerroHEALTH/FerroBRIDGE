// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The RM object an ADL 1.4 operational template constrains at each path.
//!
//! The Web Template compacts the RM levels between an archetype root and a
//! leaf (Simplified Formats, §Level Removal), so it no longer says which class
//! and which node id an `ITEM_TREE` under a `details` attribute carried, nor the
//! rubric that names it. A party is built without the composition builder, so
//! the index reads those facts from the operational template and keys them by
//! the `aqlPath` spelling the Web Template builder uses. No specification
//! governs this record: our own design.

use std::collections::BTreeMap;

use openehr_its::opt14::types::ArchetypeTerm;
use openehr_its::opt14::types::CAttribute;
use openehr_its::opt14::types::CObject;
use openehr_its::opt14::types::OperationalTemplate;

use crate::index::embedded::fixed_name;
use crate::index::embedded::step;
use crate::index::paths::AqlPath;

/// One RM object the operational template constrains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Structural {
    /// The RM class the template constrains the object to.
    pub(crate) rm_type: String,
    /// The archetype node id: the archetype id at a root, an at-code below.
    pub(crate) node_id: String,
    /// The rubric of the node id in the enclosing archetype's terminology, or
    /// the name a single-valued `name/value` constraint fixes.
    pub(crate) name: Option<String>,
}

/// Reads every node-identified object `template` constrains, keyed by its
/// `aqlPath`; the root is keyed by the empty path.
pub(super) fn structures(template: &OperationalTemplate) -> BTreeMap<AqlPath, Structural> {
    let mut found = BTreeMap::new();
    let root = &template.definition;
    let terms = root.term_definitions.as_slice();
    found.insert(
        AqlPath::new(String::new()),
        Structural {
            rm_type: root.rm_type_name.clone(),
            node_id: root.archetype_id.value.clone(),
            name: fixed_name(&root.attributes)
                .map(str::to_owned)
                .or_else(|| rubric(terms, &root.node_id)),
        },
    );
    walk(&root.attributes, "", terms, &mut found);
    found
}

/// Records the objects under `attributes`, whose owner sits at `parent` in an
/// archetype whose terminology is `terms`.
fn walk(
    attributes: &[CAttribute],
    parent: &str,
    terms: &[ArchetypeTerm],
    found: &mut BTreeMap<AqlPath, Structural>,
) {
    for attribute in attributes {
        let (name, children) = match attribute {
            CAttribute::CMultipleAttribute(multiple) => {
                (multiple.rm_attribute_name.as_str(), &multiple.children)
            }
            CAttribute::CSingleAttribute(single) => {
                (single.rm_attribute_name.as_str(), &single.children)
            }
        };
        if name == "name" {
            continue;
        }
        for child in children {
            let (rm_type, node_id, at_code, attributes, inner) = match child {
                CObject::CArchetypeRoot(root) => (
                    root.rm_type_name.as_str(),
                    root.archetype_id.value.as_str(),
                    root.node_id.as_str(),
                    &root.attributes,
                    root.term_definitions.as_slice(),
                ),
                CObject::CComplexObject(object) => (
                    object.rm_type_name.as_str(),
                    object.node_id.as_str(),
                    object.node_id.as_str(),
                    &object.attributes,
                    terms,
                ),
                _ => continue,
            };
            let fixed = fixed_name(attributes);
            let path = step(parent, name, node_id, fixed);
            if !node_id.is_empty() {
                found.insert(
                    AqlPath::new(path.clone()),
                    Structural {
                        rm_type: rm_type.to_owned(),
                        node_id: node_id.to_owned(),
                        name: fixed.map(str::to_owned).or_else(|| rubric(inner, at_code)),
                    },
                );
            }
            walk(attributes, &path, inner, found);
        }
    }
}

/// Returns the `text` of `code` in an archetype terminology.
fn rubric(terms: &[ArchetypeTerm], code: &str) -> Option<String> {
    terms
        .iter()
        .find(|term| term.code == code)
        .and_then(|term| term.items.get("text"))
        .cloned()
}
