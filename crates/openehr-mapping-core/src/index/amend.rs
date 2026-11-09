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
        if composition.template_id() != self.template_id() {
            return Err(PathError::TemplateMismatch {
                expected: self.template_id().to_owned(),
                found: composition.template_id().to_owned(),
            });
        }
        let path = self.instance_path(node, occurrences)?;
        let absent = || PathError::AbsentInstance {
            aql_path: node.aql_path().as_str().to_owned(),
        };
        let mut value = composition.into_value();
        let pointer = self
            .navigator()
            .instances(&value, &path)
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
