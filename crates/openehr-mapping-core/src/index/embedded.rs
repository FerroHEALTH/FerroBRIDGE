// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The template an ADL 1.4 operational template embeds at an archetype root.
//!
//! An OPT 1.4 `C_ARCHETYPE_ROOT` may state a `template_id`: the template the
//! root was included from (the openEHR OPT 1.4 XML schema, `C_ARCHETYPE_ROOT`).
//! The Web Template carries no such member, so the index reads it from the
//! operational template and keys it by the `aqlPath` the Web Template builder
//! gives the same node: the parent path, the attribute, and a predicate of
//! the archetype id and the name a single-valued `name/value` constraint fixes
//! (Simplified Formats, §Web Template Metadata).

use std::collections::BTreeMap;

use openehr_its::opt14::types::CAttribute;
use openehr_its::opt14::types::CObject;
use openehr_its::opt14::types::CPrimitive;
use openehr_its::opt14::types::OperationalTemplate;

use crate::index::paths::AqlPath;

/// Reads every embedded template id `template` states, keyed by the `aqlPath`
/// of its archetype root.
pub(super) fn embedded_templates(template: &OperationalTemplate) -> BTreeMap<AqlPath, String> {
    let mut found = BTreeMap::new();
    let root = &template.definition;
    if let Some(id) = root.template_id.as_ref() {
        found.insert(AqlPath::new(String::new()), id.value.clone());
    }
    visit_objects(&root.attributes, "", &mut |parent, attribute, child| {
        if let CObject::CArchetypeRoot(root) = child
            && let Some(id) = root.template_id.as_ref()
            && let Some(path) = object_path(parent, attribute, child)
        {
            found.insert(AqlPath::new(path), id.value.clone());
        }
    });
    found
}

/// Calls `visit` with the parent path, the attribute name and the object of
/// every child under `attributes`, whose owner sits at `parent`, descending
/// through every archetype root and complex object.
///
/// The `name` attribute is passed over: it constrains a node's name, which
/// the path carries as a predicate ([`fixed_name`]), and holds no node.
pub(super) fn visit_objects(
    attributes: &[CAttribute],
    parent: &str,
    visit: &mut dyn FnMut(&str, &str, &CObject),
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
            visit(parent, name, child);
            if let Some(path) = object_path(parent, name, child) {
                let attributes = match child {
                    CObject::CArchetypeRoot(root) => &root.attributes,
                    CObject::CComplexObject(object) => &object.attributes,
                    _ => continue,
                };
                visit_objects(attributes, &path, visit);
            }
        }
    }
}

/// Returns the `aqlPath` of `child` under the attribute `attribute` of the
/// node at `parent`, when it is an archetype root or a complex object.
pub(super) fn object_path(parent: &str, attribute: &str, child: &CObject) -> Option<String> {
    match child {
        CObject::CArchetypeRoot(root) => Some(step(
            parent,
            attribute,
            root.archetype_id.value.as_str(),
            fixed_name(&root.attributes),
        )),
        CObject::CComplexObject(object) => Some(step(
            parent,
            attribute,
            object.node_id.as_str(),
            fixed_name(&object.attributes),
        )),
        _ => None,
    }
}

/// Appends one `attribute[predicate]` step to `parent`, as the Web Template
/// builder spells it.
pub(super) fn step(parent: &str, attribute: &str, node_id: &str, name: Option<&str>) -> String {
    match (node_id.is_empty(), name) {
        (true, _) => format!("{parent}/{attribute}"),
        (false, Some(name)) => format!("{parent}/{attribute}[{node_id},'{name}']"),
        (false, None) => format!("{parent}/{attribute}[{node_id}]"),
    }
}

/// Returns the name a single-valued `name/value` constraint fixes among
/// `attributes`.
pub(super) fn fixed_name(attributes: &[CAttribute]) -> Option<&str> {
    attributes.iter().find_map(|attribute| {
        let CAttribute::CSingleAttribute(single) = attribute else {
            return None;
        };
        if single.rm_attribute_name != "name" {
            return None;
        }
        single.children.iter().find_map(|child| {
            let CObject::CComplexObject(text) = child else {
                return None;
            };
            text.attributes.iter().find_map(single_value)
        })
    })
}

/// Returns the one string a `value` attribute admits, when it admits exactly
/// one.
fn single_value(attribute: &CAttribute) -> Option<&str> {
    let CAttribute::CSingleAttribute(single) = attribute else {
        return None;
    };
    if single.rm_attribute_name != "value" {
        return None;
    }
    single.children.iter().find_map(|child| {
        let CObject::CPrimitiveObject(primitive) = child else {
            return None;
        };
        match primitive.item.as_deref() {
            Some(CPrimitive::CString(string)) if string.list.len() == 1 => {
                string.list.first().map(String::as_str)
            }
            _ => None,
        }
    })
}
