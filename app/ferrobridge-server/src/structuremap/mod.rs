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

/// The maps and logical models of one mapping revision.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Set {
    /// The maps, in path order.
    pub maps: Vec<syntax::StructureMap>,
    /// The logical models the maps read.
    pub logical: model::Logical,
}

/// The engine over the running mapping revision, which a reload on any
/// replica replaces whole; a run in flight finishes on the engine it took.
#[derive(Debug)]
pub struct Running {
    /// What `translate` calls, when a terminology server is configured.
    translator: Option<Arc<dyn engine::translate::Translate>>,
    /// The engine every later run reads.
    engine: RwLock<Arc<engine::Engine>>,
}

impl Default for Running {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Running {
    /// Returns no maps, every later engine running `translate` through
    /// `translator`.
    #[must_use]
    pub fn new(translator: Option<Arc<dyn engine::translate::Translate>>) -> Self {
        let engine = Arc::new(build(Set::default(), translator.as_ref()));
        Self {
            translator,
            engine: RwLock::new(engine),
        }
    }

    /// Returns the engine every later run reads.
    #[must_use]
    pub fn current(&self) -> Arc<engine::Engine> {
        Arc::clone(&self.engine.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Replaces the engine every later run reads with one over `set`.
    pub fn replace(&self, set: Set) {
        let engine = Arc::new(build(set, self.translator.as_ref()));
        *self.engine.write().unwrap_or_else(PoisonError::into_inner) = engine;
    }
}

/// Returns the engine over `set`, translating through `translator`.
fn build(set: Set, translator: Option<&Arc<dyn engine::translate::Translate>>) -> engine::Engine {
    let engine = engine::Engine::new(
        set.maps,
        model::Known {
            logical: set.logical,
        },
    );
    match translator {
        Some(translator) => engine.with_translator(Arc::clone(translator)),
        None => engine,
    }
}
