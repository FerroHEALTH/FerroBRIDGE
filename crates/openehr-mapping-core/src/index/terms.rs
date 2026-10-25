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
use openehr_its::opt14::types::CObject;
use openehr_its::opt14::types::OperationalTemplate;

use crate::index::embedded::object_path;
use crate::index::embedded::visit_objects;
use crate::index::paths::AqlPath;

/// Reads the codes every archetype root of `template` defines, keyed by the
/// `aqlPath` of the root.
pub(super) fn archetype_terms(
    template: &OperationalTemplate,
) -> BTreeMap<AqlPath, BTreeSet<String>> {
    let mut found = BTreeMap::new();
    let root = &template.definition;
    found.insert(AqlPath::new(String::new()), codes(root));
    visit_objects(&root.attributes, "", &mut |parent, attribute, child| {
        if let CObject::CArchetypeRoot(root) = child
            && let Some(path) = object_path(parent, attribute, child)
        {
            found.insert(AqlPath::new(path), codes(root));
        }
    });
    found
}

/// Returns the codes one archetype root defines.
fn codes(root: &CArchetypeRoot) -> BTreeSet<String> {
    root.term_definitions
        .iter()
        .map(|term| term.code.clone())
        .collect()
}
