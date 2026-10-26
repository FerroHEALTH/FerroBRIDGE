// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two entry points of a party program.
//!
//! A party context maps a FHIR resource to a demographic `PARTY` and back
//! (`resolve::compile::party`). Going into openEHR the run writes its values
//! as it does for a composition and builds the party through
//! [`WebTemplateIndex::build_party`], with no composition default and no
//! `ctx/` key; coming out it reads the party through the same paths a
//! composition is read with. No specification governs a party context: our
//! own design.

use fhir_types::codec::Value;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::party::CanonicalParty;

use crate::engine::context::CallContext;
use crate::engine::outcome::Outcome;
use crate::engine::seam::Seams;
use crate::model::ast::keyword::Direction;
use crate::resolve::program::Program;
use crate::tree::element::Table;

use crate::engine::traverse::Binding;
use crate::engine::traverse::Run;
use crate::engine::traverse::error::EngineError;
use crate::engine::traverse::read_out;

/// Runs a party program over a FHIR document, producing a party.
///
/// `seams` and `context` are those of
/// [`to_openehr`](crate::engine::traverse::to_openehr).
///
/// # Errors
///
/// Returns [`EngineError::CompositionProgram`] for a program that compiled no
/// party context, [`EngineError::PartyBuild`] when the values do not build a
/// party of the template, and [`EngineError`] for any element the program
/// cannot map.
pub fn to_party<T: Table + ?Sized>(
    program: &Program,
    table: &T,
    index: &WebTemplateIndex,
    document: &Value,
    seams: &Seams<'_>,
    context: &CallContext,
) -> Result<Outcome<CanonicalParty>, EngineError> {
    party_program(program)?;
    let mut run = Run::new(
        program,
        table,
        index,
        Direction::FhirToOpenehr,
        *seams,
        context,
        document.clone(),
        None,
    );
    // NOTE: basics/Variables.adoc, `$archetype` is the root as `$resource` is, so one
    // resource maps into one instance of the start archetype and its axes are bound.
    run.pins = program
        .root_axes()
        .iter()
        .map(|axis| (axis.clone(), RmPosition::first()))
        .collect();
    run.admits_context()?;
    run.split_or_run(
        program.mappings(),
        program.hierarchy(),
        program.start().as_str(),
        &Binding::default(),
    )?;
    let (values, _) = run.node_values()?;
    let built = index
        .build_party(&values)
        .map_err(|source| EngineError::PartyBuild {
            source: Box::new(source),
        })?;
    Ok(Outcome::new(built, run.warnings).with_consumed(run.consumed))
}

/// Runs a party program over a party, producing a FHIR resource.
///
/// `seams` and `context` are those of
/// [`to_fhir`](crate::engine::traverse::to_fhir).
///
/// # Errors
///
/// Returns [`EngineError::CompositionProgram`] for a program that compiled no
/// party context, and the errors of
/// [`to_fhir`](crate::engine::traverse::to_fhir) otherwise.
pub fn from_party<T: Table + ?Sized>(
    program: &Program,
    table: &T,
    index: &WebTemplateIndex,
    party: &CanonicalParty,
    seams: &Seams<'_>,
    context: &CallContext,
) -> Result<Outcome<Value>, EngineError> {
    party_program(program)?;
    read_out(program, table, index, party, seams, context, &[])
}

/// Refuses a composition program at a party entry point.
fn party_program(program: &Program) -> Result<(), EngineError> {
    match program.party() {
        Some(_) => Ok(()),
        None => Err(EngineError::CompositionProgram {
            context: String::from(program.context().as_str()),
        }),
    }
}
