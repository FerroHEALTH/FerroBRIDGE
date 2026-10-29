// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The occurrences of a context's start archetype in a composition, named
//! without running the program.
//!
//! [`super::to_fhir_each`] maps each occurrence on its own and names it by
//! its positions and its entry path. A caller that addresses one occurrence
//! later, such as the FHIR facade reading one resource of a composition,
//! needs the same names before it runs anything, so it derives the same
//! deterministic id ([`crate::engine::identity::deterministic_id`]) the
//! `$tofhir` operation answers for that occurrence. No specification governs
//! how an occurrence is named: our own design.

use fhir_types::codec::Value;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::WebTemplateIndex;

use crate::engine::context::CallContext;
use crate::engine::outcome::Outcome;
use crate::engine::seam::Seams;
use crate::engine::traverse::error::EngineError;
use crate::resolve::program::Program;
use crate::tree::element::Table;

/// One occurrence of a context's start archetype in a composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartOccurrence {
    /// The 1-based instance of each repeating axis down to the start
    /// archetype; empty when the start archetype is on no repeating axis.
    positions: Vec<u32>,
    /// The path that names the occurrence in the composition.
    path: String,
}

impl StartOccurrence {
    /// Returns the 1-based instance of each repeating axis down to the start
    /// archetype.
    #[must_use]
    pub fn positions(&self) -> &[u32] {
        &self.positions
    }

    /// Returns the path that names the occurrence in the composition
    /// ([`WebTemplateIndex::entry_path`]), the path
    /// [`super::to_fhir_each`] answers for it: the first instance at a
    /// repeating node keeps the template's predicate and a later one takes
    /// its 1-based place in the container, and a context whose start is the
    /// composition itself names [`crate::engine::identity::ROOT_PATH`].
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// Returns every occurrence of `program`'s start archetype `composition`
/// holds, in document order, exactly as [`super::to_fhir_each`] names them.
///
/// A composition holding no occurrence answers an empty list.
///
/// # Errors
///
/// Returns [`EngineError::PartyProgram`] for a party program, and
/// [`EngineError::Template`] or [`EngineError::Position`] when the template
/// cannot answer for the start archetype's axes.
pub fn occurrences(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<Vec<StartOccurrence>, EngineError> {
    super::composition_program(program)?;
    let mut found = Vec::new();
    for instance in super::root_instances(program, index, composition)? {
        // NOTE: an instance of the axes that holds no start archetype root is
        // no occurrence of it, a legitimate absence, as `to_fhir_each` reads
        // it; no specification governs this, our own design.
        let Some(path) = super::entry_path(program, index, composition, &instance)? else {
            continue;
        };
        found.push(StartOccurrence {
            positions: instance.iter().map(|position| position.get()).collect(),
            path,
        });
    }
    Ok(found)
}

/// Runs `program` over the one occurrence `occurrence` names, producing its
/// resource, as [`super::to_fhir_each`] runs each occurrence.
///
/// A caller that maps the occurrences one at a time does so to hand each run
/// seams of its own, such as an identity sink anchored at that occurrence.
///
/// # Errors
///
/// [`EngineError`] for any element the occurrence cannot map, including
/// [`EngineError::NotApplicable`] when the context's condition does not hold
/// at it.
pub fn to_fhir_at<T: Table + ?Sized>(
    program: &Program,
    table: &T,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    seams: &Seams<'_>,
    context: &CallContext,
    occurrence: &StartOccurrence,
) -> Result<Outcome<Value>, EngineError> {
    super::composition_program(program)?;
    let mut instance = Vec::with_capacity(occurrence.positions.len());
    for position in &occurrence.positions {
        instance.push(
            RmPosition::new(*position).map_err(|source| EngineError::Position {
                mapping: String::from(program.context().as_str()),
                source,
            })?,
        );
    }
    super::read_out(
        program,
        table,
        index,
        composition,
        seams,
        context,
        &instance,
    )
}
