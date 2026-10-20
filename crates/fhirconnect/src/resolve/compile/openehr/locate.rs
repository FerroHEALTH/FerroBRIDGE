// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The template node an openEHR path names, and the reference-model tail
//! below it.

use openehr_mapping_core::index::ResolvedNode;
use openehr_mapping_core::template::PathError;
use openehr_rm::v1_2::model as rm_model;
use openehr_rm::v1_2::paths::RmPath;

use crate::resolve::compile::Compiler;
use crate::resolve::compile::openehr::LocateError;
use crate::resolve::compile::openehr::Reached;
use crate::resolve::compile::openehr::constrains;
use crate::resolve::compile::openehr::names_same_place;
use crate::resolve::derive;

impl<'a> Compiler<'a> {
    /// Returns the deepest template node an openEHR path names, and the
    /// reference-model attributes below it.
    ///
    /// A Web Template carries the archetype roots and the leaves and compacts
    /// what lies between, so three forms resolve: a path that names an indexed
    /// node, a path that names an `ELEMENT` the builder compacted into its
    /// value node, and a path that reaches past the deepest indexed node into
    /// the reference model or through a structure the template compacted away.
    /// A segment the template carries nowhere and that constrains a node
    /// identity is refused. No specification governs this: our own design.
    pub(in crate::resolve::compile) fn locate(
        &self,
        path: &RmPath,
    ) -> Result<Reached<'a>, LocateError> {
        let total = path.segments.len();
        for taken in (0..=total).rev() {
            let head = path.segments.get(..taken).unwrap_or_default();
            let tail = path.segments.get(taken..).unwrap_or_default();
            let prefix = RmPath {
                absolute: path.absolute,
                segments: head.to_vec(),
            };
            let rest = RmPath {
                absolute: false,
                segments: tail.to_vec(),
            };
            let mut compacted = false;
            let node = match self.template.at_rm_path(&prefix) {
                Ok(node) => node,
                Err(PathError::AmbiguousPath { .. }) => {
                    return Err(self.ambiguity(&prefix, &self.template.all_at_rm_path(&prefix)));
                }
                Err(_) => {
                    let found: Vec<&ResolvedNode> = self
                        .compacted
                        .iter()
                        .filter(|entry| names_same_place(&prefix, &entry.0))
                        .map(|&(_, node)| node)
                        .collect();
                    match *found.as_slice() {
                        [only] => {
                            compacted = true;
                            only
                        }
                        [] => continue,
                        _ => return Err(self.ambiguity(&prefix, &found)),
                    }
                }
            };
            let constrained = rest.segments.iter().find(|segment| constrains(segment));
            if let Some(segment) = constrained
                && !self
                    .interior
                    .iter()
                    .any(|interior| names_same_place(path, interior))
            {
                if let Some((code, root)) = self.excluded_code(node, path, taken) {
                    return Err(LocateError::Excluded(format!(
                        "`{path}` names `{code}`, which the archetype at `{}` defines and the \
                         template `{}` excludes",
                        root.aql_path().as_str(),
                        self.template.template_id()
                    )));
                }
                return Err(LocateError::Unknown(format!(
                    "the template `{}` has no node at `{path}`; the deepest node it reaches is \
                     `{}` and `{}` below it names a node identity the template does not carry",
                    self.template.template_id(),
                    node.aql_path().as_str(),
                    segment.attribute
                )));
            }
            let (leaf, on_element) = match rm_tail(node.rm_type(), &rest) {
                Ok(leaf) => (leaf, false),
                Err(message) => match element_tail(node, &rest, compacted) {
                    Some(leaf) => (Some(leaf), true),
                    None => {
                        return Err(LocateError::Unknown(format!(
                            "the template `{}` reaches `{}` and `{path}` walks `{rest}` below \
                             it: {message}",
                            self.template.template_id(),
                            node.aql_path().as_str()
                        )));
                    }
                },
            };
            return Ok(Reached {
                node,
                tail: rest,
                leaf,
                on_element,
            });
        }
        Err(LocateError::Unknown(format!(
            "the template `{}` has no node at `{path}`",
            self.template.template_id()
        )))
    }

    /// Builds the refusal for a path that names more than one template node.
    fn ambiguity(&self, path: &RmPath, candidates: &[&ResolvedNode]) -> LocateError {
        let named: Vec<&str> = candidates
            .iter()
            .map(|node| node.flat_id().as_str())
            .collect();
        LocateError::Ambiguous(format!(
            "`{path}` names {} nodes of the template `{}`: {}",
            named.len(),
            self.template.template_id(),
            named.join(", ")
        ))
    }
}

/// Returns the class a tail ends on when it names attributes of the
/// `ELEMENT` the Web Template compacted into `node`, its value.
///
/// A path that names an `ELEMENT` reaches the node of its value, whose
/// `aqlPath` ends in `/value`; the element is a `LOCATABLE` (openEHR RM, the
/// `LOCATABLE` class), and `FHIRReference.adoc` §Dedicated Node writes
/// `$openehrRoot/links` below one. A tail the value's class carries is the
/// value's, so only a tail it does not carry is tried on the element.
fn element_tail(node: &ResolvedNode, tail: &RmPath, compacted: bool) -> Option<String> {
    let holds_value = node.rm_path().segments.last().is_some_and(|last| {
        last.attribute == ELEMENT_VALUE && last.predicate.archetype_node_id.is_none()
    });
    if !compacted || !holds_value {
        return None;
    }
    rm_tail(ELEMENT_CLASS, tail).ok().flatten()
}

/// The reference-model class of a leaf of an item structure.
const ELEMENT_CLASS: &str = "ELEMENT";

/// The attribute of an `ELEMENT` that holds its data value.
const ELEMENT_VALUE: &str = "value";

/// Checks the reference-model attributes an openEHR path walks below the
/// deepest template node it reaches.
///
/// A Web Template carries the nodes an archetype constrains and stops there, so
/// everything below the deepest one is plain reference model. The attribute
/// model `openehr-rm` generates from the RM BMM is the oracle for it
/// (<https://docs.rs/openehr-rm/0.0.69/openehr_rm/v1_2/model/fn.attribute.html>):
/// an attribute is looked up on the node's type and on the concrete subtypes of
/// it, because a declared type may be abstract (`ELEMENT.value` is
/// `DATA_VALUE`) and the attribute then belongs to one of its descendants.
///
/// A generic node (`DV_INTERVAL<DV_QUANTITY>`) is walked by its class, and a
/// bound below it is its parameter ([`derive::attribute_class`]).
///
/// A type the model does not carry stops the walk rather than refusing it: the
/// mapping is then checked as far as the model reaches and no further.
///
/// Returns the class of the last attribute the tail names, `None` for an
/// empty tail and for a walk the model stopped before its end. The class is
/// the declared type, or its one concrete descendant when the declared type is
/// abstract and has exactly one.
pub(super) fn rm_tail(rm_type: &str, tail: &RmPath) -> Result<Option<String>, String> {
    let mut candidates = concrete_forms(derive::base_class(rm_type));
    if candidates.is_empty() {
        return Ok(None);
    }
    let mut leaf: Option<&'static str> = None;
    for (position, segment) in tail.segments.iter().enumerate() {
        let attribute = segment.attribute.as_str();
        let mut declared: Option<&'static str> = None;
        let mut next: Vec<&'static str> = Vec::new();
        for candidate in &candidates {
            let Some(found) = rm_model::attribute(candidate, attribute) else {
                continue;
            };
            let held = if position == 0 {
                derive::attribute_class(rm_type, found)
            } else {
                found.declared_type
            };
            declared.get_or_insert(held);
            for form in concrete_forms(held) {
                if !next.contains(&form) {
                    next.push(form);
                }
            }
        }
        let Some(declared) = declared else {
            return Err(format!(
                "`{attribute}` is no attribute of `{}` in the openEHR reference model",
                candidates.join("`, `")
            ));
        };
        leaf = Some(declared);
        if next.is_empty() {
            let last = position.saturating_add(1) == tail.segments.len();
            return Ok(last.then(|| String::from(declared)));
        }
        candidates = next;
    }
    Ok(leaf.map(concrete_leaf))
}

/// Returns the one concrete form of an abstract class, or the class itself.
fn concrete_leaf(declared: &'static str) -> String {
    let Some(class) = rm_model::class(declared) else {
        return String::from(declared);
    };
    if !class.is_abstract {
        return String::from(declared);
    }
    let concrete: Vec<&str> = class
        .descendants
        .iter()
        .copied()
        .filter(|name| rm_model::class(name).is_some_and(|found| !found.is_abstract))
        .collect();
    match *concrete.as_slice() {
        [only] => String::from(only),
        _ => String::from(declared),
    }
}

/// Returns the reference-model class plus every concrete class below it.
///
/// An empty result means the reference model carries no class of that name,
/// which is what stops [`rm_tail`] rather than refusing.
pub(super) fn concrete_forms(rm_type: &str) -> Vec<&'static str> {
    let Some(class) = rm_model::class(rm_type) else {
        return Vec::new();
    };
    let mut forms: Vec<&'static str> = vec![class.name];
    for descendant in class.descendants {
        if !forms.contains(descendant) {
            forms.push(descendant);
        }
    }
    forms
}
