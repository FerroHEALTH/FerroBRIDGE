// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node codes the archetype behind each archetype root of an ADL 1.4
//! operational template defines.
//!
//! An OPT 1.4 `C_ARCHETYPE_ROOT` carries the `term_definitions` of the
//! archetype it roots (the openEHR OPT 1.4 XML schema, `C_ARCHETYPE_ROOT`),
//! and those cover every at-code the archetype defines, including the codes
//! of the nodes the template leaves out. A Web Template carries only the
//! nodes the template keeps, so a code the archetype defines and the template
//! carries nowhere is a node the template excludes. The map is keyed by the
//! `aqlPath` the Web Template builder gives the root, the key
//! [`super::embedded`] uses.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use openehr_its::opt14::types::CArchetypeRoot;
use openehr_its::opt14::types::CAttribute;
use openehr_its::opt14::types::CObject;
use openehr_its::opt14::types::OperationalTemplate;

use crate::index::embedded::fixed_name;
use crate::index::embedded::step;
use crate::index::paths::AqlPath;

/// Reads the codes every archetype root of `template` defines, keyed by the
/// `aqlPath` of the root.
pub(super) fn archetype_terms(
    template: &OperationalTemplate,
) -> BTreeMap<AqlPath, BTreeSet<String>> {
    let mut found = BTreeMap::new();
    let root = &template.definition;
    found.insert(AqlPath::new(String::new()), codes(root));
    walk(&root.attributes, "", &mut found);
    found
}

/// Returns the codes one archetype root defines.
fn codes(root: &CArchetypeRoot) -> BTreeSet<String> {
    root.term_definitions
        .iter()
        .map(|term| term.code.clone())
        .collect()
}

/// Records the codes of every archetype root under `attributes`, whose owner
/// sits at `parent`.
fn walk(attributes: &[CAttribute], parent: &str, found: &mut BTreeMap<AqlPath, BTreeSet<String>>) {
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
            match child {
                CObject::CArchetypeRoot(root) => {
                    let path = step(
                        parent,
                        name,
                        root.archetype_id.value.as_str(),
                        fixed_name(&root.attributes),
                    );
                    found.insert(AqlPath::new(path.clone()), codes(root));
                    walk(&root.attributes, &path, found);
                }
                CObject::CComplexObject(object) => {
                    let path = step(
                        parent,
                        name,
                        object.node_id.as_str(),
                        fixed_name(&object.attributes),
                    );
                    walk(&object.attributes, &path, found);
                }
                _ => {}
            }
        }
    }
}
