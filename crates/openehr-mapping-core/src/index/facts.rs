// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What an ADL 1.4 operational template states about a node that the Web
//! Template document has no member for, read from the nodes the
//! `openehr-sdt` builder made.
//!
//! The builder keeps these facts on the node it builds from each constraint
//! object (`WebTemplateNode::facts`, and the terminology of each coded
//! option), walking an OPT 1.4 `T_COMPLEX_OBJECT` as the `C_COMPLEX_OBJECT`
//! it extends (openEHR OPT 1.4 `Template.xsd`). The index keys them by the
//! node's `aqlPath`, so it never walks the operational template a second time.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use openehr_sdt::flat::webtemplate::model::WebTemplateNode;

use crate::index::ordinals::OrdinalOption;
use crate::index::paths::AqlPath;

/// The `rm_type` of a Web Template node that holds a `DV_ORDINAL`.
const DV_ORDINAL: &str = "DV_ORDINAL";

/// The facts of every node of one template, keyed by `aqlPath`.
#[derive(Debug, Default)]
pub(super) struct Facts {
    /// The template each archetype root states it was included from.
    pub(super) embedded: BTreeMap<AqlPath, String>,
    /// The codes the archetype behind each archetype root defines.
    pub(super) archetype_terms: BTreeMap<AqlPath, BTreeSet<String>>,
    /// The options each `C_DV_ORDINAL` node lists.
    pub(super) ordinals: BTreeMap<AqlPath, Vec<OrdinalOption>>,
}

/// Reads the facts of `tree` and of every node under it.
///
/// An archetype root is a node whose facts carry the archetype's terms: an
/// OPT 1.4 archetype always defines at least the code of its own root
/// concept (AM 1.4 Archetype Object Model, `ARCHETYPE.concept`, defined in
/// the ontology), so a root never carries an empty set.
pub(super) fn read(tree: &WebTemplateNode) -> Facts {
    let mut facts = Facts::default();
    let mut stack = vec![tree];
    while let Some(node) = stack.pop() {
        let path = AqlPath::new(node.aql_path.clone());
        if let Some(id) = &node.facts.embedded_template_id {
            facts.embedded.insert(path.clone(), id.clone());
        }
        if !node.facts.archetype_terms.is_empty() {
            let codes = node.facts.archetype_terms.keys().cloned().collect();
            facts.archetype_terms.insert(path.clone(), codes);
        }
        if node.rm_type == DV_ORDINAL {
            let options = ordinal_options(node);
            if !options.is_empty() {
                facts.ordinals.insert(path, options);
            }
        }
        stack.extend(node.children.iter().chain(&node.alternatives));
    }
    facts
}

/// Returns the options a `DV_ORDINAL` node lists, in template order: each
/// coded option that carries its ordinal and the terminology of its symbol.
fn ordinal_options(node: &WebTemplateNode) -> Vec<OrdinalOption> {
    node.inputs
        .iter()
        .flat_map(|input| &input.list)
        .filter_map(|option| {
            let value = option.ordinal?;
            let terminology = option.terminology.as_deref()?;
            Some(OrdinalOption::new(value, &option.value, terminology))
        })
        .collect()
}
