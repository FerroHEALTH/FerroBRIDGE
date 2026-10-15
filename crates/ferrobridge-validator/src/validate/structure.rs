// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `StructureDefinition`'s snapshot, indexed by element id.

use std::collections::BTreeMap;
use std::sync::Arc;

use fhir_types::r4::element_definition::ElementDefinition;
use fhir_types::r4::structure_definition::StructureDefinition;

use super::ValidateError;
use crate::set::PackageId;

/// The snapshot of one `StructureDefinition`, its elements by id.
///
/// The snapshots the packages publish write a slice name into an element id
/// after a `:`, and a re-slice name after a `/`
/// (<https://hl7.org/fhir/R4/elementdefinition.html#id>, §2.30.0.3), so a slice and the
/// elements inside it have ids of their own. An element without an id is
/// indexed by its path, where the first element of the path wins.
#[derive(Debug)]
pub(crate) struct Structure {
    url: String,
    definition: Arc<StructureDefinition>,
    by_id: BTreeMap<String, usize>,
    origin: PackageId,
}

impl Structure {
    /// Indexes `definition`, which SHALL carry a snapshot.
    pub(crate) fn new(
        definition: Arc<StructureDefinition>,
        origin: PackageId,
    ) -> Result<Self, ValidateError> {
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
            if id.is_empty() {
                continue;
            }
            by_id.entry(id).or_insert(index);
        }
        Ok(Self {
            url,
            definition,
            by_id,
            origin,
        })
    }

    /// Returns the package that carries the definition.
    pub(crate) const fn origin(&self) -> PackageId {
        self.origin
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

    /// Returns the element with `id`.
    pub(crate) fn element(&self, id: &str) -> Option<&ElementDefinition> {
        let index = *self.by_id.get(id)?;
        self.definition.snapshot.as_ref()?.element.get(index)
    }

    /// Returns the slices of the slicing entry `entry`, in snapshot order,
    /// each with its id.
    ///
    /// The slices of `Patient.identifier` are `Patient.identifier:name`, and
    /// the re-slices of the slice `Patient.identifier:name` are
    /// `Patient.identifier:name/reslice` (profiling.html §5.1.0.14).
    pub(crate) fn slices(&self, entry: &str) -> Vec<(&str, &ElementDefinition)> {
        let last = entry.rsplit('.').next().unwrap_or(entry);
        let separator = if last.contains(':') { '/' } else { ':' };
        let Some(snapshot) = &self.definition.snapshot else {
            return Vec::new();
        };
        snapshot
            .element
            .iter()
            .filter(|element| element.slice_name.is_some())
            .filter_map(|element| {
                let id = element.id.as_deref()?;
                let name = id.strip_prefix(entry)?.strip_prefix(separator)?;
                let simple = !name.is_empty() && !name.contains(['.', ':', '/']);
                simple.then_some((id, element))
            })
            .collect()
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
