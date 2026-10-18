// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR release a package set is read as and an instance is validated
//! in.
//!
//! A release names its generated `StructureDefinition`, `ElementDefinition`,
//! `ValueSet` and `CodeSystem`, which the validator reads through the release-neutral
//! `fhir_types::definition` traits, its element table, and the FHIRPath type
//! model its invariants are checked and evaluated against. [`R4`] and
//! [`Stu3`] are the two releases the validator walks.

use std::fmt;

use ferrobridge_fhirpath::model::Model;
use fhir_types::codec::Json;
use fhir_types::definition::CodeSystemRead;
use fhir_types::definition::ElementDefinitionRead;
use fhir_types::definition::StructureDefinitionRead;
use fhir_types::definition::ValueSetRead;
use fhir_types::schema::Schemas;

/// Keeps the set of releases closed to this crate.
mod sealed {
    /// The marker every release implements.
    pub trait Sealed {}
}

/// A FHIR release the validator reads packages and instances in.
pub trait Release:
    sealed::Sealed + fmt::Debug + Clone + Copy + Default + Send + Sync + 'static
{
    /// The release's generated `StructureDefinition`.
    type StructureDefinition: StructureDefinitionRead<Element = Self::ElementDefinition>
        + Json
        + fmt::Debug
        + Send
        + Sync;

    /// The release's generated `ElementDefinition`.
    type ElementDefinition: ElementDefinitionRead + fmt::Debug + Send + Sync;

    /// The release's generated `ValueSet`.
    type ValueSet: ValueSetRead + Json + fmt::Debug + Send + Sync;

    /// The release's generated `CodeSystem`.
    type CodeSystem: CodeSystemRead + Json + fmt::Debug + Send + Sync;

    /// The release's name in messages, `R4` or `STU3`.
    const NAME: &'static str;

    /// The codes an extension's definition types its contexts with: R4
    /// `extension-context-type`, STU3 `extension-context`.
    const CONTEXT_TYPES: &'static [&'static str];

    /// The release's element table.
    fn schemas() -> &'static Schemas;

    /// The FHIRPath type model an invariant is checked and evaluated
    /// against.
    fn model() -> &'static dyn Model;
}

/// FHIR R4 (4.0.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct R4;

impl sealed::Sealed for R4 {}

impl Release for R4 {
    type StructureDefinition = fhir_types::r4::structure_definition::StructureDefinition;
    type ElementDefinition = fhir_types::r4::element_definition::ElementDefinition;
    type ValueSet = fhir_types::r4::value_set::ValueSet;
    type CodeSystem = fhir_types::r4::code_system::CodeSystem;

    const NAME: &'static str = "R4";
    const CONTEXT_TYPES: &'static [&'static str] = &["fhirpath", "element", "extension"];

    fn schemas() -> &'static Schemas {
        &fhir_types::r4::schema::SCHEMAS
    }

    fn model() -> &'static dyn Model {
        &ferrobridge_fhirpath::model::r4::R4
    }
}

/// FHIR STU3 (3.0.2).
///
/// STU3 states its invariants as FHIRPath, and the evaluator reads them as
/// FHIRPath N1 over the STU3 types, the reading the `ferrobridge-fhirpath`
/// STU3 model records with its ground.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stu3;

impl sealed::Sealed for Stu3 {}

impl Release for Stu3 {
    type StructureDefinition = fhir_types::stu3::structure_definition::StructureDefinition;
    type ElementDefinition = fhir_types::stu3::element_definition::ElementDefinition;
    type ValueSet = fhir_types::stu3::value_set::ValueSet;
    type CodeSystem = fhir_types::stu3::code_system::CodeSystem;

    const NAME: &'static str = "STU3";
    const CONTEXT_TYPES: &'static [&'static str] = &["resource", "datatype", "extension"];

    fn schemas() -> &'static Schemas {
        &fhir_types::stu3::schema::SCHEMAS
    }

    fn model() -> &'static dyn Model {
        &ferrobridge_fhirpath::model::stu3::Stu3
    }
}
