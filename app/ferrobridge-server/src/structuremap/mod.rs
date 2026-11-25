// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR Mapping Language: a map parsed from its text and run over a
//! source instance into FHIR R4 (FHIR R4 §7.7, mapping-language.html, and
//! the `StructureMap` resource, structuremap.html).
//!
//! A source that sends its own JSON is described by a logical model, a
//! `StructureDefinition` of `kind = logical`, and a map turns an instance of
//! it into R4 resources. The engine holds no knowledge of any source.

pub mod engine;
pub mod expression;
pub mod model;
pub mod openapi;
pub mod parse;
pub mod syntax;

use std::sync::{Arc, PoisonError, RwLock};

/// The maps of the running mapping revision, which a reload on any replica
/// replaces whole; a run in flight finishes on the maps it took.
#[derive(Debug, Default)]
pub struct Running {
    maps: RwLock<Arc<[syntax::StructureMap]>>,
}

impl Running {
    /// Returns the maps every later run reads.
    #[must_use]
    pub fn current(&self) -> Arc<[syntax::StructureMap]> {
        Arc::clone(&self.maps.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Replaces the maps every later run reads.
    pub fn replace(&self, maps: Vec<syntax::StructureMap>) {
        *self.maps.write().unwrap_or_else(PoisonError::into_inner) = Arc::from(maps);
    }
}
