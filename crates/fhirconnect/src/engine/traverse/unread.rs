// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The elements of a composition that a run out to FHIR left unread.
//!
//! A context names the elements it maps, and a composition may hold more.
//! The run records each template node instance it read a value of, and once
//! it is done every archetype element the composition holds at the mapped
//! occurrence and the run did not read is declared as
//! [`Loss::Unread`], so the resource never drops content with no trace. No
//! specification governs the declaration: our own design.

use std::collections::BTreeSet;

use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::ResolvedNode;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::FlatId;
use openehr_rm::v1_2::model;
use openehr_sdt::flat::path::FlatKey;
use openehr_sdt::flat::path::Segment;

use crate::engine::outcome::Location;
use crate::engine::outcome::Loss;
use crate::engine::outcome::Warning;
use crate::resolve::derive;
use crate::resolve::program::Program;

use crate::engine::traverse::error::EngineError;

/// The attributes of an entry that hold recorded content with no archetype
/// node identifier of their own: `ACTIVITY.timing`, "Timing of the activity
/// of the instruction", and `INSTRUCTION.expiry_time`
/// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/ehr.html#_activity_class>).
const CONTENT_ATTRIBUTES: &[&str] = &["timing", "expiry_time"];

/// One instance of a template node: its flat id and the 1-based position of
/// each repeating node down to it.
pub(super) type Instance = (FlatId, Vec<u32>);

/// Returns the instance `positions` names of the node `flat_id`.
pub(super) fn instance_of(flat_id: &FlatId, positions: &[RmPosition]) -> Instance {
    (
        flat_id.clone(),
        positions.iter().map(|position| position.get()).collect(),
    )
}

/// Returns a warning for each archetype element `composition` holds below
/// the program's start archetype at `instance` that is not in `read`, in
/// document order.
///
/// An element is a template node with an archetype node identifier
/// (openEHR AOM 1.4 §Node identifiers, an `at` code) or one of the
/// [`CONTENT_ATTRIBUTES`], whose class is a data value, so the attributes the
/// reference model gives every entry, its `language`, `encoding` and
/// `subject`, are never declared.
///
/// # Errors
///
/// [`EngineError::Template`] when the template cannot flatten the
/// composition or name a node it holds.
pub(super) fn unread(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    instance: &[RmPosition],
    read: &BTreeSet<Instance>,
) -> Result<Vec<Warning>, EngineError> {
    let refuse = |node: &str| {
        let node = String::from(node);
        move |source| EngineError::Template {
            mapping: String::from(program.context().as_str()),
            node,
            source: Box::new(source),
        }
    };
    let flat = index.flatten(composition).map_err(refuse(""))?;
    let root = program.root().map(|root| format!("{}/", root.as_str()));
    let pinned: Vec<u32> = instance.iter().map(|position| position.get()).collect();
    let mut seen: BTreeSet<Instance> = BTreeSet::new();
    let mut warnings = Vec::new();
    for key in flat.keys() {
        let Some(held) = held_instance(key, root.as_deref(), &pinned) else {
            continue;
        };
        if read.contains(&held) || !seen.insert(held.clone()) {
            continue;
        }
        let Some((node, node_id, attribute)) = element_of(index, &held, read) else {
            continue;
        };
        let mut at = Vec::with_capacity(held.1.len());
        for &position in &held.1 {
            at.push(
                RmPosition::new(position).map_err(|source| EngineError::Position {
                    mapping: String::from(program.context().as_str()),
                    source,
                })?,
            );
        }
        let mut path = index
            .entry_path(composition, node, &at)
            .map_err(refuse(node.flat_id().as_str()))?
            .map_or_else(
                || String::from(node.aql_path().as_str()),
                |path| path.to_string(),
            );
        if let Some(attribute) = attribute {
            path.push('/');
            path.push_str(attribute);
        }
        warnings.push(Warning::new(
            Loss::Unread {
                node: String::from(node_id),
            },
            Location::default().with_openehr(path),
        ));
    }
    Ok(warnings)
}

/// Returns the node instance a FLAT key names, when it lies below `root` at
/// the `pinned` occurrence.
fn held_instance(key: &str, root: Option<&str>, pinned: &[u32]) -> Option<Instance> {
    // NOTE: a key that does not parse as a node path is a context or an RM attribute of
    // the FLAT vocabulary, which carries no archetype element, so it is legitimately skipped.
    let parsed = FlatKey::parse(key).ok()?;
    if parsed.is_ctx() || parsed.segments.iter().any(Segment::is_rm_attribute) {
        return None;
    }
    let names: Vec<&str> = parsed
        .segments
        .iter()
        .map(|segment| segment.name.as_str())
        .collect();
    let flat_id = FlatId::new(names.join("/"));
    if root.is_some_and(|root| !format!("{}/", flat_id.as_str()).starts_with(root)) {
        return None;
    }
    let positions: Vec<u32> = parsed
        .segments
        .iter()
        .filter_map(|segment| segment.index)
        .map(|index| index.saturating_add(1))
        .collect();
    positions
        .starts_with(pinned)
        .then_some((flat_id, positions))
}

/// Returns the template node `held` names, its node identifier and, for one
/// of the [`CONTENT_ATTRIBUTES`], the attribute below the node, `None` for no
/// archetype element or one the run read.
fn element_of<'index>(
    index: &'index WebTemplateIndex,
    held: &Instance,
    read: &BTreeSet<Instance>,
) -> Option<(&'index ResolvedNode, &'index str, Option<&'static str>)> {
    if let Ok(node) = index.node_by_flat_id(&held.0) {
        let node_id = node
            .node_id()
            .filter(|id| id.starts_with("at") || id.starts_with("id"))?;
        if model::is_a(derive::base_class(node.rm_type()), derive::STRUCTURAL) {
            return None;
        }
        return Some((node, node_id, None));
    }
    // NOTE: the Web Template carries no node for an entry attribute such as
    // `ACTIVITY.timing`; one of the content attributes is named below its parent node.
    let (parent, name) = held.0.as_str().rsplit_once('/')?;
    let &name = CONTENT_ATTRIBUTES.iter().find(|&&known| known == name)?;
    // NOTE: FLAT indexes only a repeating node (Simplified Formats §Instance
    // Indexing), so the attribute's positions are its parent's.
    let below = (FlatId::new(parent), held.1.clone());
    if read.contains(&below) {
        return None;
    }
    let node = index.node_by_flat_id(&below.0).ok()?;
    Some((node, name, Some(name)))
}
