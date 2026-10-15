// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `StructureDefinition`'s snapshot, indexed by element id.

use std::collections::BTreeMap;
use std::sync::Arc;

use fhir_types::r4::element_definition::ElementDefinition;
use fhir_types::r4::structure_definition::StructureDefinition;

use super::ValidateError;

/// The snapshot of one `StructureDefinition`, its elements by id.
///
/// Only the elements outside every slice are indexed: the snapshots the
/// packages publish write a slice name into an element id after a `:`, so an
/// id holding one names a slice or an element inside it, and slices are
/// reported as unevaluated rather than walked.
#[derive(Debug)]
pub(crate) struct Structure {
    url: String,
    definition: Arc<StructureDefinition>,
    by_id: BTreeMap<String, usize>,
}

impl Structure {
    /// Indexes `definition`, which SHALL carry a snapshot.
    pub(crate) fn new(definition: Arc<StructureDefinition>) -> Result<Self, ValidateError> {
        let url = definition.url.value.clone().unwrap_or_default();
        let Some(snapshot) = &definition.snapshot else {
            return Err(ValidateError::NoSnapshot { url });
        };
        let mut by_id = BTreeMap::new();
        for (index, element) in snapshot.element.iter().enumerate() {
            let id = element
                .id
                .clone()
                .or_else(|| element.path.value.clone())
                .unwrap_or_default();
            if id.is_empty() || id.contains(':') || element.slice_name.is_some() {
                continue;
            }
            by_id.entry(id).or_insert(index);
        }
        Ok(Self {
            url,
            definition,
            by_id,
        })
    }

    /// Returns the canonical URL of the definition.
    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// Returns the definition.
    pub(crate) fn definition(&self) -> &StructureDefinition {
        &self.definition
    }

    /// Returns the type the definition constrains.
    pub(crate) fn type_name(&self) -> &str {
        self.definition.r#type.value.as_deref().unwrap_or_default()
    }

    /// Returns the element with `id`, outside every slice.
    pub(crate) fn element(&self, id: &str) -> Option<&ElementDefinition> {
        let index = *self.by_id.get(id)?;
        self.definition.snapshot.as_ref()?.element.get(index)
    }

    /// Answers whether the snapshot defines an element below `id`.
    pub(crate) fn has_children(&self, id: &str) -> bool {
        let prefix = format!("{id}.");
        self.by_id
            .range(prefix.clone()..)
            .next()
            .is_some_and(|(key, _)| key.starts_with(&prefix))
    }
}
