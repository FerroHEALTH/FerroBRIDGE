// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The nodes an operational template excludes from the archetype it roots.

use core::str::FromStr;

use openehr_mapping_core::index::ResolvedNode;
use openehr_mapping_core::path::MappingPath;
use openehr_mapping_core::position::Located;
use openehr_rm::v1_2::model as rm_model;
use openehr_rm::v1_2::paths::PathSegment;
use openehr_rm::v1_2::paths::RmPath;

use crate::model::ast::keyword::Variable;
use crate::resolve::compile::Compiler;
use crate::resolve::compile::Scope;
use crate::resolve::compile::openehr::LocateError;
use crate::resolve::compile::openehr::locate::concrete_forms;
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
        let end = (taken..path.segments.len()).find(|&end| {
            let prefix = RmPath {
                absolute: path.absolute,
                segments: path.segments.get(..=end).unwrap_or_default().to_vec(),
            };
            !self
                .interior
                .iter()
                .any(|interior| names_same_place(&prefix, interior))
        })?;
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
            .find_map(|form| rm_model::attribute(form, &segment.attribute))
            .map(|attribute| attribute.container);
        match container {
            None => true,
            Some(rm_model::Container::None) => {
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
        let resolved = parsed.resolve(&anchor).ok()?;
        match self.locate(&resolved) {
            Err(LocateError::Excluded(message)) => Some(message),
            _ => None,
        }
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
