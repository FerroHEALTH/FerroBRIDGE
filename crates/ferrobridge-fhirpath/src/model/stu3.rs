// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR STU3 (3.0.2) type model, over the generated STU3 element table.
//!
//! STU3 states its invariants as FHIRPath expressions
//! (<https://hl7.org/fhir/STU3/conformance-rules.html>); the evaluator reads
//! them as FHIRPath N1 over the STU3 types, so `ofType()`, `is`, `as` and
//! every path resolve against what STU3 defines.
//!
//! STU3 links the unversioned FHIRPath specification
//! (`http://hl7.org/fluentpath`) and publishes no FHIRPath page of its own,
//! so the language read is the evaluator's N1 with the FHIR additions of the
//! R4 page; no specification governs that pairing: our own design.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use fhir_types::schema::Schemas;

use super::{Model, TypeKind, primitive_types};

/// The STU3 primitive types, read once from the table.
static PRIMITIVES: LazyLock<BTreeMap<&'static str, TypeKind>> =
    LazyLock::new(|| primitive_types(&fhir_types::stu3::schema::SCHEMAS));

/// The FHIR STU3 type model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stu3;

impl Model for Stu3 {
    fn schemas(&self) -> &'static Schemas {
        &fhir_types::stu3::schema::SCHEMAS
    }

    fn primitives(&self) -> &BTreeMap<&'static str, TypeKind> {
        &PRIMITIVES
    }
}
