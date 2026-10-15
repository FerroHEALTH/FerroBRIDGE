// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What an evaluation calls out to: the caller's reference resolution,
//! terminology, profile validation and structure definitions.
//!
//! FHIR R4 defines `resolve()`, `memberOf()`, `subsumes()`,
//! `subsumedBy()`, `conformsTo()`, `elementDefinition()` and `slice()` over
//! things outside the document (<https://hl7.org/fhir/R4/fhirpath.html#2.9.1.5>).
//! The evaluator makes no call of its own: each is a trait the caller
//! supplies on the [`Environment`](crate::eval::Environment). With no seam
//! supplied, calling the function is an
//! [`EvalError::NoSeam`](crate::eval::error::EvalError::NoSeam), never an
//! empty result, and a seam's own failure travels as its source.

use std::error::Error;
use std::fmt;

use fhir_types::codec::Value;

/// A failure a seam reports, carried as the source of the evaluation error.
pub type SeamError = Box<dyn Error + Send + Sync + 'static>;

/// Where a reference is resolved from (FHIR R4 §2.9.1.4).
#[derive(Debug, Clone, Copy)]
pub struct ResolveContext<'r> {
    /// `%resource`, the resource holding the reference.
    pub resource: Option<&'r Value>,
    /// `%rootResource`, the resource that contains `%resource`.
    pub root_resource: Option<&'r Value>,
}

/// Locates the target of a reference for `resolve()`.
pub trait Resolver: fmt::Debug {
    /// The resource `reference` names, `None` when it names nothing the
    /// resolver can find; R4 drops such an item from the result.
    ///
    /// # Errors
    ///
    /// Returns the resolver's own failure, which ends the evaluation.
    fn resolve(
        &self,
        reference: &str,
        context: ResolveContext<'_>,
    ) -> Result<Option<Value>, SeamError>;
}

/// A coding: the parts of a FHIR `Coding` a terminology question reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coding<'c> {
    /// `Coding.system`.
    pub system: Option<&'c str>,
    /// `Coding.version`.
    pub version: Option<&'c str>,
    /// `Coding.code`.
    pub code: Option<&'c str>,
}

/// What `memberOf()` asks about (FHIR R4 §2.9.1.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Coded<'c> {
    /// A `code` or a string, its system left to the value set.
    Code(&'c str),
    /// A `Coding`.
    Coding(Coding<'c>),
    /// A `CodeableConcept`'s codings; a member when any of them is.
    Concept(Vec<Coding<'c>>),
}

/// Answers the terminology questions of `memberOf()`, `subsumes()` and
/// `subsumedBy()`.
pub trait Terminology: fmt::Debug {
    /// Whether `coded` is a member of the value set `value_set`.
    ///
    /// # Errors
    ///
    /// Returns the terminology's own failure, including a value set it cannot
    /// resolve, which ends the evaluation.
    fn member_of(&self, coded: &Coded<'_>, value_set: &str) -> Result<bool, SeamError>;

    /// Whether `ancestor` subsumes `descendant` or is equivalent to it.
    ///
    /// # Errors
    ///
    /// Returns the terminology's own failure, including codings of systems
    /// whose relationship it does not know, which ends the evaluation.
    fn subsumes(&self, ancestor: &Coding<'_>, descendant: &Coding<'_>) -> Result<bool, SeamError>;
}

/// Validates an element against a profile for `conformsTo()`.
pub trait Profiles: fmt::Debug {
    /// Whether `value`, of the FHIR type `type_name`, conforms to the
    /// structure definition `structure`.
    ///
    /// # Errors
    ///
    /// Returns the validator's own failure, including a structure it cannot
    /// resolve, which ends the evaluation.
    fn conforms_to(
        &self,
        value: &Value,
        type_name: Option<&str>,
        structure: &str,
    ) -> Result<bool, SeamError>;
}

/// Reads structure definitions for `elementDefinition()` and `slice()`.
pub trait Definitions: fmt::Debug {
    /// The `ElementDefinition` of the element path `path`, as JSON.
    ///
    /// # Errors
    ///
    /// Returns the reader's own failure, which ends the evaluation.
    fn element_definition(&self, path: &str) -> Result<Option<Value>, SeamError>;

    /// Whether `value`, of the FHIR type `type_name`, belongs to the slice
    /// `slice` the structure definition `structure` defines.
    ///
    /// # Errors
    ///
    /// Returns the reader's own failure, including a structure or a slice it
    /// cannot find, which ends the evaluation (FHIR R4 §2.9.1.5).
    fn in_slice(
        &self,
        structure: &str,
        slice: &str,
        value: &Value,
        type_name: Option<&str>,
    ) -> Result<bool, SeamError>;
}
