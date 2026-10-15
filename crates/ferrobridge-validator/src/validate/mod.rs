// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The validator: an instance walked over the R4 element table, held to the
//! snapshot of the profile it is validated against and of every datatype and
//! extension definition it reaches.
//!
//! The element table `fhir_types::r4::schema::SCHEMAS` is the base structure:
//! which members an object may carry, their base cardinality, their types,
//! and the hop a `contentReference` makes. The snapshot elements of the
//! `StructureDefinitions` in play add the profile's constraints: cardinality,
//! the types a choice may take, `fixed[x]`, `pattern[x]`, `maxLength`,
//! slicing and the profiles an element's types name. Invariants, bindings,
//! target profiles and claimed profiles are listed in the report as
//! unevaluated.

mod codes;
mod extension;
mod listing;
mod matching;
mod primitive;
mod profiles;
mod resolver;
mod slicing;
mod structure;
mod walk;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use fhir_types::codec::EncodeError;
use fhir_types::codec::Value;

use crate::report::Report;
use crate::set::PackageSet;
use crate::set::ResolveError;
use codes::Enumeration;
use primitive::PrimitiveRule;
use resolver::Contained;
use resolver::Resolver;
use structure::Structure;

/// The canonical base of the definitions FHIR R4 core publishes.
const CORE: &str = "http://hl7.org/fhir/StructureDefinition/";

/// Validates R4 resources against the profiles of a package set.
#[derive(Debug)]
pub struct Validator<'set> {
    set: &'set PackageSet,
    structures: Mutex<BTreeMap<String, Arc<Structure>>>,
    primitives: Mutex<BTreeMap<String, Arc<PrimitiveRule>>>,
    codes: Mutex<BTreeMap<String, Arc<Enumeration>>>,
    resolver: Box<dyn Resolver>,
}

/// Why a validation could not run to the end.
///
/// An instance that does not conform is never an error: it is a
/// [`Report`] with issues. An error says the package set cannot answer.
#[derive(Debug, thiserror::Error)]
pub enum ValidateError {
    /// A definition the validation needs did not resolve.
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    /// A definition carries no snapshot, which the validator reads.
    #[error("the StructureDefinition {url} carries no snapshot")]
    NoSnapshot {
        /// The definition's canonical URL.
        url: String,
    },
    /// The `regex` extension of a primitive's definition does not compile.
    #[error("the lexical form of {type_code} does not compile")]
    Regex {
        /// The primitive type.
        type_code: String,
        /// The compile error.
        #[source]
        source: regex::Error,
    },
    /// An element definition's `fixed[x]` or `pattern[x]` has no JSON form.
    #[error("the value of {element} in {structure} has no JSON form")]
    Encode {
        /// The definition's canonical URL.
        structure: String,
        /// The element id.
        element: String,
        /// The encode error.
        #[source]
        source: EncodeError,
    },
    /// The element table names a type it does not carry.
    #[error("the R4 element table carries no type {name}")]
    UnknownType {
        /// The type name.
        name: String,
    },
}

impl<'set> Validator<'set> {
    /// Creates a validator over `set`.
    #[must_use]
    pub fn new(set: &'set PackageSet) -> Self {
        Self {
            set,
            structures: Mutex::new(BTreeMap::new()),
            primitives: Mutex::new(BTreeMap::new()),
            codes: Mutex::new(BTreeMap::new()),
            resolver: Box::new(Contained),
        }
    }

    /// Validates `resource` against the core definition of its type.
    ///
    /// Each profile the resource claims in `meta.profile` is listed as
    /// unevaluated.
    ///
    /// # Errors
    ///
    /// Returns [`ValidateError`] when a definition the validation reaches does
    /// not resolve, carries no snapshot, or cannot be read.
    pub fn validate(&self, resource: &Value) -> Result<Report, ValidateError> {
        walk::Walk::new(self).run(resource, None)
    }

    /// Validates `resource` against the profile `profile` names, a canonical
    /// with an optional `|version`.
    ///
    /// # Errors
    ///
    /// Returns [`ValidateError`] when the profile or a definition the
    /// validation reaches does not resolve, carries no snapshot, or cannot be
    /// read.
    pub fn validate_against(
        &self,
        resource: &Value,
        profile: &str,
    ) -> Result<Report, ValidateError> {
        walk::Walk::new(self).run(resource, Some(profile))
    }

    /// Returns the indexed snapshot of the definition `canonical` names.
    fn structure(&self, canonical: &str) -> Result<Arc<Structure>, ValidateError> {
        if let Some(found) = self
            .structures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(canonical)
        {
            return Ok(Arc::clone(found));
        }
        let structure = Arc::new(Structure::new(self.set.structure_definition(canonical)?)?);
        self.structures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(canonical.to_owned(), Arc::clone(&structure));
        Ok(structure)
    }

    /// Returns the core definition of the type `name`.
    fn core(&self, name: &str) -> Result<Arc<Structure>, ValidateError> {
        self.structure(&format!("{CORE}{name}"))
    }

    /// Returns the resolver a `resolve()` in a discriminator path uses.
    fn resolver(&self) -> &dyn Resolver {
        self.resolver.as_ref()
    }

    /// Returns the codes of the value set `canonical` names, when the
    /// package set enumerates them.
    fn codes(&self, canonical: &str) -> Result<Arc<Enumeration>, ValidateError> {
        if let Some(found) = self
            .codes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(canonical)
        {
            return Ok(Arc::clone(found));
        }
        let enumeration = Arc::new(codes::enumerate(self.set, canonical)?);
        self.codes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(canonical.to_owned(), Arc::clone(&enumeration));
        Ok(enumeration)
    }

    /// Returns the lexical rule of the primitive type `code`.
    fn primitive(&self, code: &str) -> Result<Arc<PrimitiveRule>, ValidateError> {
        if let Some(found) = self
            .primitives
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(code)
        {
            return Ok(Arc::clone(found));
        }
        let rule = Arc::new(PrimitiveRule::read(&*self.core(code)?, code)?);
        self.primitives
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(code.to_owned(), Arc::clone(&rule));
        Ok(rule)
    }
}
