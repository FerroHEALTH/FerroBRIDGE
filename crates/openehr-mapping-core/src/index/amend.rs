// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An edit of one node instance of a built composition, validated again.
//!
//! The FLAT builder spells a node only through the attributes Simplified
//! Formats gives it. It builds every abstract `EVENT` node as a
//! `POINT_EVENT`, and takes `/width` and `/math_function` only on an
//! `INTERVAL_EVENT` (master05 §`INTERVAL_EVENT`), so an interval event at an
//! `EVENT` node is written into the built canonical JSON instead. The result is
//! validated against the template like any composition that reaches the
//! bridge. No specification governs the edit: our own design.

use openehr_rm::v1_2::paths::RmPath;
use openehr_sdt::flat::navigate::Instance;
use serde_json::Map;
use serde_json::Value;

use crate::composition::CanonicalComposition;
use crate::composition::RmPosition;
use crate::index::ResolvedNode;
use crate::index::WebTemplateIndex;
use crate::template::PathError;

impl WebTemplateIndex {
    /// Returns `composition` with one instance of `node` edited by `edit`,
    /// validated against this template.
    ///
    /// The instance is named the way [`WebTemplateIndex::read`] names it, and
    /// `edit` receives the canonical JSON object the instance is.
    ///
    /// # Errors
    ///
    /// Returns [`PathError::TemplateMismatch`] when the composition was built
    /// against another template, [`PathError::OccurrenceCount`] when the
    /// occurrences do not match the repeating nodes above the node,
    /// [`PathError::AbsentInstance`] when the composition holds no object at
    /// the instance, and [`PathError::InvalidComposition`] when the edited
    /// document does not validate.
    pub fn amend(
        &self,
        composition: CanonicalComposition,
        node: &ResolvedNode,
        occurrences: &[RmPosition],
        edit: impl FnOnce(&mut Map<String, Value>),
    ) -> Result<CanonicalComposition, PathError> {
        let path = self.instance_in(composition.template_id(), node, occurrences)?;
        self.amend_at(composition, node, &path, edit)
    }

    /// Returns `composition` with the object that holds one instance of
    /// `node`, one attribute above it, edited by `edit`, validated against
    /// this template.
    ///
    /// The Web Template keeps the `time` of a single `EVENT` it collapses
    /// into its `OBSERVATION` and no node of the event
    /// ([`WebTemplateIndex::read_holder`]), so an interval event there is
    /// written through the object that holds that leaf.
    ///
    /// # Errors
    ///
    /// The errors of [`WebTemplateIndex::amend`], with
    /// [`PathError::AbsentInstance`] also for a node at the root, which has no
    /// holder.
    pub fn amend_holder(
        &self,
        composition: CanonicalComposition,
        node: &ResolvedNode,
        occurrences: &[RmPosition],
        edit: impl FnOnce(&mut Map<String, Value>),
    ) -> Result<CanonicalComposition, PathError> {
        let mut path = self.instance_in(composition.template_id(), node, occurrences)?;
        if path.segments.pop().is_none() {
            return Err(PathError::AbsentInstance {
                aql_path: node.aql_path().as_str().to_owned(),
            });
        }
        self.amend_at(composition, node, &path, edit)
    }

    /// Edits the object at `path` of `composition` and validates the result.
    fn amend_at(
        &self,
        composition: CanonicalComposition,
        node: &ResolvedNode,
        path: &RmPath,
        edit: impl FnOnce(&mut Map<String, Value>),
    ) -> Result<CanonicalComposition, PathError> {
        let absent = || PathError::AbsentInstance {
            aql_path: node.aql_path().as_str().to_owned(),
        };
        let mut value = composition.into_value();
        let pointer = self
            .navigator()
            .instances(&value, path)
            .first()
            .map(Instance::pointer)
            .ok_or_else(absent)?;
        let object = value
            .pointer_mut(&pointer)
            .and_then(Value::as_object_mut)
            .ok_or_else(absent)?;
        edit(object);
        self.accept(value)
    }
}
