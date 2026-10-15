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
//! slicing, the profiles an element's types name and terminology bindings.
//! Invariants, target profiles and claimed profiles are listed in the report
//! as unevaluated, and so is every binding the package set cannot decide,
//! which [`crate::report::Report::resolve_bindings`] can send to a
//! terminology server.

mod extension;
mod listing;
mod matching;
mod primitive;
mod profiles;
mod resolver;
mod slicing;
mod structure;
mod terminology;
mod walk;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use fhir_types::codec::EncodeError;
use fhir_types::codec::Value;

use crate::report::Reason;
use crate::report::Report;
use crate::set::PackageId;
use crate::set::PackageSet;
use crate::set::ResolveError;
use primitive::PrimitiveRule;
use resolver::Contained;
use resolver::Resolver;
use structure::Structure;
use terminology::index::CodeIndex;

/// The canonical base of the definitions FHIR R4 core publishes.
const CORE: &str = "http://hl7.org/fhir/StructureDefinition/";

/// A canonical as one package references it: the cache key of a resolution.
type Scoped = (String, Option<PackageId>);

/// Validates R4 resources against the profiles of a package set.
#[derive(Debug)]
pub struct Validator<'set> {
    set: &'set PackageSet,
    structures: Mutex<BTreeMap<Scoped, Arc<Structure>>>,
    primitives: Mutex<BTreeMap<String, Arc<PrimitiveRule>>>,
    code_systems: Mutex<BTreeMap<Scoped, Result<Arc<CodeIndex>, Reason>>>,
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
            code_systems: Mutex::new(BTreeMap::new()),
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

    /// Returns the indexed snapshot of the definition `canonical` names, as
    /// the definition `from` references it: within the dependency closure of
    /// `from`'s package first.
    fn structure(
        &self,
        canonical: &str,
        from: Option<&Structure>,
    ) -> Result<Arc<Structure>, ValidateError> {
        let scope = from.map(Structure::origin);
        let key = (canonical.to_owned(), scope);
        if let Some(found) = self
            .structures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return Ok(Arc::clone(found));
        }
        let (definition, origin) = self.set.structure_definition_in(canonical, scope)?;
        let structure = Arc::new(Structure::new(definition, origin)?);
        self.structures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, Arc::clone(&structure));
        Ok(structure)
    }

    /// Returns the core definition of the type `name`.
    fn core(&self, name: &str) -> Result<Arc<Structure>, ValidateError> {
        self.structure(&format!("{CORE}{name}"), None)
    }

    /// Returns the resolver a `resolve()` in a discriminator path uses.
    fn resolver(&self) -> &dyn Resolver {
        self.resolver.as_ref()
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
