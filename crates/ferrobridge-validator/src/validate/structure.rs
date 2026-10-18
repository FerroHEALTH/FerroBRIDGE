// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `StructureDefinition`'s snapshot, indexed by element id.

use std::collections::BTreeMap;
use std::sync::Arc;

use fhir_types::definition::ElementDefinitionRead;
use fhir_types::definition::StructureDefinitionRead;

use super::Element;
use super::ValidateError;
use crate::release::Release;
use crate::set::PackageId;

/// The snapshot of one `StructureDefinition`, its elements by id.
///
/// The snapshots the packages publish write a slice name into an element id
/// after a `:`, and a re-slice name after a `/`
/// (<https://hl7.org/fhir/R4/elementdefinition.html#id>, §2.30.0.3), so a slice and the
/// elements inside it have ids of their own. An element without an id is
/// indexed by its path, where the first element of the path wins.
#[derive(Debug)]
pub(crate) struct Structure<R: Release> {
    url: String,
    definition: Arc<R::StructureDefinition>,
    by_id: BTreeMap<String, usize>,
    origin: PackageId,
}

impl<R: Release> Structure<R> {
    /// Indexes `definition`, which SHALL carry a snapshot.
    pub(crate) fn new(
        definition: Arc<R::StructureDefinition>,
        origin: PackageId,
    ) -> Result<Self, ValidateError> {
        let url = definition.url().unwrap_or_default().to_owned();
        let Some(snapshot) = definition.snapshot() else {
            return Err(ValidateError::NoSnapshot { url });
        };
        let mut by_id = BTreeMap::new();
        for (index, element) in snapshot.iter().enumerate() {
            let id = element
                .element_id()
                .or_else(|| element.path())
                .unwrap_or_default()
                .to_owned();
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
    pub(crate) fn definition(&self) -> &R::StructureDefinition {
        &self.definition
    }

    /// Returns the type the definition constrains.
    pub(crate) fn type_name(&self) -> &str {
        self.definition.type_name().unwrap_or_default()
    }

    /// Returns the element with `id`.
    pub(crate) fn element(&self, id: &str) -> Option<&Element<R>> {
        let index = *self.by_id.get(id)?;
        self.definition.snapshot()?.get(index)
    }

    /// Returns the slices of the slicing entry `entry`, in snapshot order,
    /// each with its id.
    ///
    /// The slices of `Patient.identifier` are `Patient.identifier:name`, and
    /// the re-slices of the slice `Patient.identifier:name` are
    /// `Patient.identifier:name/reslice` (profiling.html §5.1.0.14).
    pub(crate) fn slices(&self, entry: &str) -> Vec<(&str, &Element<R>)> {
        let last = entry.rsplit('.').next().unwrap_or(entry);
        let separator = if last.contains(':') { '/' } else { ':' };
        let Some(snapshot) = self.definition.snapshot() else {
            return Vec::new();
        };
        snapshot
            .iter()
            .filter(|element| element.slice_name().is_some())
            .filter_map(|element| {
                let id = element.element_id()?;
                let name = id.strip_prefix(entry)?.strip_prefix(separator)?;
                let simple = !name.is_empty() && !name.contains(['.', ':', '/']);
                simple.then_some((id, element))
            })
            .collect()
    }

    /// Answers whether the snapshot defines an element whose id starts with
    /// `stem`: the element, a slice of it, a child, or for a `choice` one of
    /// its type forms (`Observation.valueQuantity`).
    pub(crate) fn mentions(&self, stem: &str, choice: bool) -> bool {
        self.by_id
            .range(stem.to_owned()..)
            .map(|(key, _)| key)
            .take_while(|key| key.starts_with(stem))
            .any(|key| {
                key.strip_prefix(stem)
                    .and_then(|rest| rest.chars().next())
                    .is_none_or(|next| {
                        matches!(next, '.' | ':' | '[') || (choice && next.is_ascii_uppercase())
                    })
            })
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
