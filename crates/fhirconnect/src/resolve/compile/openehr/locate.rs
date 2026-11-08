// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The template node an openEHR path names, and the reference-model tail
//! below it.

use openehr_mapping_core::index::ResolvedNode;
use openehr_mapping_core::template::PathError;
use openehr_rm::v1_2::model;
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
                    let candidates = self.template.all_at_rm_path(&prefix);
                    match *carrying(&candidates, &rest).as_slice() {
                        [only] => only,
                        _ => return Err(self.ambiguity(&prefix, &candidates)),
                    }
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

/// Returns the nodes among `candidates` whose class carries `tail`.
///
/// A polymorphic `ELEMENT` gives each data type it admits a value node at one
/// path, and a tail below that path names the attributes of one class: a
/// `normal_range` below the value of an element admitting `DV_QUANTITY` and
/// `DV_TEXT` names the quantity, which alone is a `DV_ORDERED`. An empty tail
/// selects no alternative. No specification governs this: our own design.
fn carrying<'a>(candidates: &[&'a ResolvedNode], tail: &RmPath) -> Vec<&'a ResolvedNode> {
    if tail.segments.is_empty() {
        return Vec::new();
    }
    candidates
        .iter()
        .copied()
        .filter(|node| rm_tail(node.rm_type(), tail).is_ok())
        .collect()
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
    let mut holder = String::from(rm_type);
    for (position, segment) in tail.segments.iter().enumerate() {
        let attribute = segment.attribute.as_str();
        let mut declared: Option<&'static str> = None;
        let mut typed: Option<String> = None;
        let mut next: Vec<&'static str> = Vec::new();
        for candidate in &candidates {
            let Some(found) = model::attribute(candidate, attribute) else {
                continue;
            };
            let held = derive::attribute_class(&holder, found);
            declared.get_or_insert(held);
            typed.get_or_insert_with(|| instantiated(&holder, held, found));
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
        holder = typed.unwrap_or_else(|| String::from(declared));
        if next.is_empty() {
            let last = position.saturating_add(1) == tail.segments.len();
            return Ok(last.then(|| String::from(declared)));
        }
        candidates = next;
    }
    Ok(leaf.map(concrete_leaf))
}

/// Returns the class `held` with the one type argument `attribute` declares
/// it with (`DV_INTERVAL<DV_QUANTITY>` for `DV_QUANTITY.normal_range`), so a
/// bound below it reads as that argument.
///
/// An attribute of a generic `holder` that declares a generic class with no
/// argument is typed by the holder's argument where the two classes bound
/// their one parameter alike: `REFERENCE_RANGE<T: DV_ORDERED>.range` is a
/// `DV_INTERVAL<T>`
/// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/data_types.html#_reference_range_class>),
/// so the range of a `REFERENCE_RANGE<DV_QUANTITY>` is a
/// `DV_INTERVAL<DV_QUANTITY>`.
fn instantiated(holder: &str, held: &str, attribute: &model::RmAttribute) -> String {
    match *attribute.type_params {
        [ref only] if held == attribute.declared_type && only.params.is_empty() => {
            format!("{held}<{}>", only.name)
        }
        // NOTE: the RM 1.2.0 BMM declares `REFERENCE_RANGE.range` a plain `DV_INTERVAL`
        // property, so the parameter the RM text gives it is read from the holder.
        [] if held == attribute.declared_type => {
            elided_argument(holder, held).map_or_else(|| String::from(held), String::from)
        }
        _ => String::from(held),
    }
}

/// Returns `held<A>` when `holder` is `C<A>`, `C` and `held` each have one
/// generic parameter with the same bound, and `A` conforms to it.
fn elided_argument(holder: &str, held: &str) -> Option<String> {
    let argument = derive::type_argument(holder)?;
    let bound_of = |class: &str| match *model::class(class)?.generic_params {
        [ref only] => only.conforms_to,
        _ => None,
    };
    let bound = bound_of(derive::base_class(holder))?;
    (bound_of(held) == Some(bound) && model::is_a(argument, bound))
        .then(|| format!("{held}<{argument}>"))
}

/// Returns the one concrete form of an abstract class, or the class itself.
fn concrete_leaf(declared: &'static str) -> String {
    let Some(class) = model::class(declared) else {
        return String::from(declared);
    };
    if !class.is_abstract {
        return String::from(declared);
    }
    let concrete: Vec<&str> = class
        .descendants
        .iter()
        .copied()
        .filter(|name| model::class(name).is_some_and(|found| !found.is_abstract))
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
    let Some(class) = model::class(rm_type) else {
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

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use openehr_rm::v1_2::paths::RmPath;

    use super::rm_tail;

    /// Returns the class `tail` ends on below a node of `rm_type`.
    fn leaf(rm_type: &str, tail: &str) -> Option<String> {
        let path = RmPath::from_str(tail).expect("the tail parses");
        rm_tail(rm_type, &path).expect("the tail walks")
    }

    #[test]
    fn a_bound_of_another_reference_range_is_the_quantity_the_range_belongs_to() {
        // RM 1.1.0 data_types §REFERENCE_RANGE: `range` is a DV_INTERVAL<T>.
        assert_eq!(
            leaf("DV_QUANTITY", "other_reference_ranges/range/lower").as_deref(),
            Some("DV_QUANTITY")
        );
        assert_eq!(
            leaf("DV_COUNT", "other_reference_ranges/range/upper").as_deref(),
            Some("DV_COUNT")
        );
        assert_eq!(
            leaf("DV_QUANTITY", "other_reference_ranges/meaning").as_deref(),
            Some("DV_TEXT")
        );
    }

    #[test]
    fn an_element_of_a_list_reads_as_the_class_the_list_holds() {
        assert_eq!(leaf("DV_TEXT", "mappings").as_deref(), Some("TERM_MAPPING"));
        assert_eq!(
            leaf("DV_TEXT", "mappings/target").as_deref(),
            Some("CODE_PHRASE")
        );
    }
}
