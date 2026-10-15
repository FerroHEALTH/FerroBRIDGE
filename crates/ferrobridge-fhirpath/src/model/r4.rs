// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R4 (4.0.1) type model, over the generated R4 element table.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use fhir_types::schema::Schemas;

use super::{Model, TypeKind, primitive_types};

/// The R4 primitive types, read once from the table.
static PRIMITIVES: LazyLock<BTreeMap<&'static str, TypeKind>> =
    LazyLock::new(|| primitive_types(&fhir_types::r4::schema::SCHEMAS));

/// The FHIR R4 type model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct R4;

impl Model for R4 {
    fn schemas(&self) -> &'static Schemas {
        &fhir_types::r4::schema::SCHEMAS
    }

    fn primitives(&self) -> &BTreeMap<&'static str, TypeKind> {
        &PRIMITIVES
    }
}
