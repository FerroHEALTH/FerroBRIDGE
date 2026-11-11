// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The nodes an operational template excludes from the archetype it roots.

use core::str::FromStr;

use openehr_mapping_core::index::ResolvedNode;
use openehr_mapping_core::index::matching::node_id_matches;
use openehr_mapping_core::path::MappingPath;
use openehr_mapping_core::position::Located;
use openehr_rm::v1_2::model;
use openehr_rm::v1_2::paths::PathSegment;
use openehr_rm::v1_2::paths::Predicate;
use openehr_rm::v1_2::paths::RmPath;

use crate::engine::rm::tail_walk::concrete_forms;
use crate::model::ast::keyword::Variable;
use crate::resolve::compile::Compiler;
use crate::resolve::compile::Scope;
use crate::resolve::compile::openehr::LocateError;
use crate::resolve::compile::openehr::locate::rm_tail;
use crate::resolve::compile::openehr::names_same_place;

impl<'a> Compiler<'a> {
    /// Returns the at-code of the first segment of `path` past its deepest
    /// template node that the template carries nowhere, with the archetype
    /// root that defines it, when the archetype defines it.
    ///
    /// Model mappings "contain some mappings that are not part of the
    /// corresponding Profile/Templates, as they have (0..0) occurrences. These
    /// are ignored and not executed"
    /// (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/model-mappings.adoc`,
    /// §Mapping 0..0 fields). The archetype's own term definitions tell such a
    /// node from one the archetype never had, so a misspelled code is still
    /// refused
    /// ([`openehr_mapping_core::index::WebTemplateIndex::archetype_defines`]).
    pub(super) fn excluded_code(
        &self,
        node: &'a ResolvedNode,
        path: &RmPath,
        taken: usize,
    ) -> Option<(String, &'a ResolvedNode)> {
        let end = self.first_uncarried(path, taken)?;
        let code = path
            .segments
            .get(end)?
            .predicate
            .archetype_node_id
            .as_deref()?;
        if !code.starts_with("at")
            || self.carries_code_here(path, end)
            || self.single_attribute_taken(node, path, taken, end)
        {
            return None;
        }
        let root = self.enclosing_archetype_root(node)?;
        (self.template.archetype_defines(root, code) == Some(true)).then(|| (code.to_owned(), root))
    }

    /// Returns the index of the first segment of `path` at or past `taken`
    /// whose prefix names no interior node of the template.
    fn first_uncarried(&self, path: &RmPath, taken: usize) -> Option<usize> {
        (taken..path.segments.len()).find(|&end| {
            let prefix = RmPath {
                absolute: path.absolute,
                segments: path.segments.get(..=end).unwrap_or_default().to_vec(),
            };
            !self
                .interior
                .iter()
                .any(|interior| names_same_place(&prefix, interior))
        })
    }

    /// Returns the archetype id of the first segment of `path` past its
    /// deepest template node, with the path of the attribute holding it, when
    /// the template keeps an open slot there that admits the archetype and
    /// fills it with none of it.
    ///
    /// An unfilled slot holds no archetype in this template, so a model
    /// mapping's slot into it addresses nodes "not part of the corresponding
    /// Profile/Templates", which "are ignored and not executed"
    /// (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/model-mappings.adoc`,
    /// §Mapping 0..0 fields). A slot that does not admit the archetype leaves
    /// the path refused, so a mistyped archetype id is still reported.
    pub(super) fn unfilled_slot(&self, path: &RmPath, taken: usize) -> Option<(String, RmPath)> {
        let end = self.first_uncarried(path, taken)?;
        let segment = path.segments.get(end)?;
        let archetype = segment.predicate.archetype_node_id.as_deref()?;
        if !archetype.starts_with(ARCHETYPE_ID_PREFIX) {
            return None;
        }
        let mut attribute = path.segments.get(..=end).unwrap_or_default().to_vec();
        if let Some(last) = attribute.last_mut() {
            last.predicate = Predicate::default();
        }
        let attribute = RmPath {
            absolute: path.absolute,
            segments: attribute,
        };
        self.template
            .open_slots()
            .iter()
            .any(|slot| names_same_place(&attribute, slot.attribute()) && slot.admits(archetype))
            .then(|| (archetype.to_owned(), attribute))
    }

    /// Returns whether the template carries a node at the place of the segment
    /// at `end` under some other name, which makes the path a name mismatch
    /// rather than an excluded node.
    fn carries_code_here(&self, path: &RmPath, end: usize) -> bool {
        let mut segments = path.segments.get(..=end).unwrap_or_default().to_vec();
        if let Some(last) = segments.last_mut() {
            last.predicate.name_value = None;
        }
        let unnamed = RmPath {
            absolute: path.absolute,
            segments,
        };
        self.interior
            .iter()
            .any(|interior| names_same_place(&unnamed, interior))
            || !self.template.all_at_rm_path(&unnamed).is_empty()
    }

    /// Returns whether the segment at `end` sits in a single-valued attribute
    /// that already holds a node of the template, or in an attribute the
    /// reference model does not settle.
    ///
    /// A single-valued attribute holds one object, so a node id there that is
    /// not the one the template keeps names the wrong place rather than an
    /// excluded sibling (`data[at0012]` where the archetype's data is
    /// `data[at0001]`). No specification governs this: our own design.
    fn single_attribute_taken(
        &self,
        node: &ResolvedNode,
        path: &RmPath,
        taken: usize,
        end: usize,
    ) -> bool {
        let Some(segment) = path.segments.get(end) else {
            return true;
        };
        let between = RmPath {
            absolute: false,
            segments: path.segments.get(taken..end).unwrap_or_default().to_vec(),
        };
        let parent = if between.segments.is_empty() {
            Some(node.rm_type().to_owned())
        } else {
            rm_tail(node.rm_type(), &between).ok().flatten()
        };
        let Some(parent) = parent else {
            return true;
        };
        let container = concrete_forms(&parent)
            .into_iter()
            .find_map(|form| model::attribute(form, &segment.attribute))
            .map(|attribute| attribute.container);
        match container {
            None => true,
            Some(model::Container::None) => {
                let written = path.segments.get(..end).unwrap_or_default();
                self.interior
                    .iter()
                    .chain(self.template.nodes().map(ResolvedNode::rm_path))
                    .any(|carried| holds_attribute(written, carried, &segment.attribute))
            }
            Some(_) => false,
        }
    }

    /// Returns the archetype root `node` lies in: the nearest archetype root
    /// on its `aqlPath`, or the template root when none lies between.
    fn enclosing_archetype_root(&self, node: &'a ResolvedNode) -> Option<&'a ResolvedNode> {
        let segments = &node.rm_path().segments;
        let Some(last) = segments.iter().rposition(|segment| {
            segment
                .predicate
                .archetype_node_id
                .as_deref()
                .is_some_and(|id| id.starts_with(ARCHETYPE_ID_PREFIX))
        }) else {
            return Some(self.template.root());
        };
        let prefix = RmPath {
            absolute: node.rm_path().absolute,
            segments: segments.get(..=last)?.to_vec(),
        };
        self.template.at_rm_path(&prefix).ok()
    }

    /// Returns the absolute path `written` names under `scope`, `None` when it
    /// does not parse or is anchored at no openEHR path.
    fn anchored(written: &Located<String>, scope: &Scope) -> Option<RmPath> {
        let parsed = MappingPath::from_str(written.value()).ok()?;
        let anchor = match parsed
            .variable()
            .map(openehr_mapping_core::path::PathVariable::name)
            .map(Variable::from_str)
        {
            None | Some(Ok(Variable::OpenehrRoot)) => scope.openehr.clone(),
            Some(Ok(Variable::Archetype)) => scope.archetype.clone(),
            Some(Ok(Variable::Composition)) => RmPath {
                absolute: true,
                segments: Vec::new(),
            },
            Some(_) => return None,
        };
        parsed.resolve(&anchor).ok()
    }

    /// Returns why the template excludes the node `written` names, when it
    /// does.
    ///
    /// The path is anchored and located as [`Compiler::openehr_target`] does,
    /// with every other outcome left to it, so a path that fails for any other
    /// reason is refused there with its own diagnostic.
    pub(in crate::resolve::compile) fn excluded(
        &self,
        written: &Located<String>,
        scope: &Scope,
    ) -> Option<String> {
        let resolved = Self::anchored(written, scope)?;
        match self.locate(&resolved) {
            Err(LocateError::Excluded(message)) => Some(message),
            _ => None,
        }
    }

    /// Returns whether the place `written` names under `scope` lies where the
    /// template stops nesting one of `archetypes`.
    ///
    /// It does when the template carries no node there, whether or not it
    /// keeps an open slot there, and the first step it carries no node for is
    /// an archetype root of one of `archetypes`. Any
    /// other unreached place, a mistyped code or a wrong attribute, answers
    /// `false`, so it is refused where [`Compiler::openehr_target`] locates it.
    pub(in crate::resolve::compile) fn nesting_ends(
        &self,
        written: &Located<String>,
        scope: &Scope,
        archetypes: &[&str],
    ) -> bool {
        let Some(resolved) = Self::anchored(written, scope) else {
            return false;
        };
        if !matches!(
            self.locate(&resolved),
            Err(LocateError::Unknown(_) | LocateError::Excluded(_))
        ) {
            return false;
        }
        let unreached = (0..resolved.segments.len()).find(|&end| {
            let prefix = RmPath {
                absolute: resolved.absolute,
                segments: resolved.segments.get(..=end).unwrap_or_default().to_vec(),
            };
            self.template.at_rm_path(&prefix).is_err()
                && !self
                    .interior
                    .iter()
                    .any(|interior| names_same_place(&prefix, interior))
        });
        unreached
            .and_then(|end| resolved.segments.get(end))
            .and_then(|segment| segment.predicate.archetype_node_id.as_deref())
            .is_some_and(|carried| {
                archetypes
                    .iter()
                    .any(|archetype| node_id_matches(archetype, carried))
            })
    }
}

/// The prefix every archetype id carries in the node-id predicate of an
/// archetype root (openEHR BASE Release 1.2.0, `ARCHETYPE_ID`, whose
/// `rm_publisher` the Web Template builder always writes as `openEHR`).
const ARCHETYPE_ID_PREFIX: &str = "openEHR-";

/// Whether `carried` walks the place `written` names and then the attribute
/// `attribute`, whatever node it selects there.
fn holds_attribute(written: &[PathSegment], carried: &RmPath, attribute: &str) -> bool {
    let Some(step) = carried.segments.get(written.len()) else {
        return false;
    };
    let prefix = RmPath {
        absolute: carried.absolute,
        segments: written.to_vec(),
    };
    let held = RmPath {
        absolute: carried.absolute,
        segments: carried
            .segments
            .get(..written.len())
            .unwrap_or_default()
            .to_vec(),
    };
    step.attribute == attribute && names_same_place(&prefix, &held)
}
