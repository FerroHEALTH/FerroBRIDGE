// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ferrobridge-fhirpath` seams an invariant's evaluation calls, filled
//! from the validation (<https://hl7.org/fhir/R4/fhirpath.html>, §2.9.1.5):
//! `resolve()` through the validator's resolver, `memberOf()` through the
//! value sets of the package set, and `conformsTo()` through a validation of
//! its own. A question the run cannot answer is a seam failure, so the
//! invariant is listed as unevaluated, never passed.

use std::fmt;

use ferrobridge_fhirpath::seam;
use ferrobridge_fhirpath::seam::ResolveContext;
use ferrobridge_fhirpath::seam::SeamError;
use fhir_types::codec::Value;

use crate::report::Reason;
use crate::set::PackageId;
use crate::set::ResolveError;
use crate::validate::ValidateError;
use crate::validate::Validator;
use crate::validate::resolver::Resolution;
use crate::validate::resolver::from_bundle;
use crate::validate::terminology::Coded;
use crate::validate::terminology::Coding;
use crate::validate::terminology::Form;
use crate::validate::terminology::Membership;
use crate::validate::walk::Cursor;
use crate::validate::walk::Walk;

/// How many `conformsTo()` validations may nest inside one another.
const DEPTH: u8 = 4;

/// A question a seam cannot answer from the validation.
#[derive(Debug, thiserror::Error)]
enum Unanswered {
    /// The walk's own reason.
    #[error("{0}")]
    Reason(Reason),
    /// Subsumption is not read from the package set.
    #[error("subsumption is not evaluated from the package set")]
    Subsumption,
    /// `conformsTo()` on a value that is neither a resource nor a complex
    /// datatype.
    #[error("conformsTo() is evaluated on a resource or a complex datatype only")]
    NotComplex,
    /// `conformsTo()` nested deeper than [`DEPTH`].
    #[error("conformsTo() nests more than {DEPTH} validations deep")]
    TooDeep,
}

/// Resolves a reference for `resolve()` from inside the resources the walk
/// is in.
pub(super) struct References<'v, 'set, 's, 'i> {
    validator: &'v Validator<'set>,
    scope: &'s [&'i Value],
}

impl<'v, 'set, 's, 'i> References<'v, 'set, 's, 'i> {
    /// Creates the resolver of `validator` over `scope`.
    pub(super) const fn new(validator: &'v Validator<'set>, scope: &'s [&'i Value]) -> Self {
        Self { validator, scope }
    }
}

impl fmt::Debug for References<'_, '_, '_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("References")
            .field("scope", &self.scope.len())
            .finish_non_exhaustive()
    }
}

impl seam::Resolver for References<'_, '_, '_, '_> {
    fn resolve(
        &self,
        reference: &str,
        _context: ResolveContext<'_>,
    ) -> Result<Option<Value>, SeamError> {
        let mut found = self.validator.resolver().resolve(reference, self.scope);
        // NOTE: no specification governs this: our own design; on the Bundle element no entry
        // holds the reference, so only an absolute one resolves, against the entries' fullUrls.
        if matches!(found, Resolution::OutOfScope)
            && let Some(&innermost) = self.scope.last()
            && innermost.get("resourceType").and_then(Value::as_str) == Some("Bundle")
        {
            found = from_bundle(innermost, reference);
        }
        match found {
            Resolution::Found { target, .. } => Ok(Some(target.clone())),
            // NOTE: fhirpath.html §2.9.1.5: an item that "does not resolve to a resource" is
            // ignored, for a reference into the resources searched that names none of them.
            Resolution::Missing => Ok(None),
            Resolution::OutOfScope => {
                Err(Box::new(Unanswered::Reason(Reason::ReferenceOutOfScope)))
            }
            Resolution::Ambiguous => Err(Box::new(Unanswered::Reason(Reason::AmbiguousReference))),
        }
    }
}

/// Answers `memberOf()` from the value sets of the package set, as the
/// definition carrying the invariant references them.
pub(super) struct Terminology<'v, 'set> {
    validator: &'v Validator<'set>,
    scope: PackageId,
}

impl<'v, 'set> Terminology<'v, 'set> {
    /// Creates the terminology of `validator` for a definition of the
    /// package `scope`.
    pub(super) const fn new(validator: &'v Validator<'set>, scope: PackageId) -> Self {
        Self { validator, scope }
    }
}

impl fmt::Debug for Terminology<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Terminology")
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
}

/// The parts of a `Coding` a value set membership reads.
const fn coding<'c>(position: usize, coding: &seam::Coding<'c>) -> Coding<'c> {
    Coding {
        system: coding.system,
        version: coding.version,
        code: coding.code,
        display: None,
        position,
    }
}

impl seam::Terminology for Terminology<'_, '_> {
    fn member_of(&self, coded: &seam::Coded<'_>, value_set: &str) -> Result<bool, SeamError> {
        let question = match coded {
            seam::Coded::Code(code) => Coded::code(code),
            seam::Coded::Coding(one) => Coded {
                form: Form::Coding,
                codes: vec![coding(0, one)],
                text: None,
            },
            seam::Coded::Concept(codings) => Coded {
                form: Form::CodeableConcept,
                codes: codings
                    .iter()
                    .enumerate()
                    .map(|(position, one)| coding(position, one))
                    .collect(),
                text: None,
            },
        };
        match self.validator.holds(value_set, self.scope, &question)? {
            Membership::Member => Ok(true),
            Membership::NotMember => Ok(false),
            Membership::Unknown(reason) => Err(Box::new(Unanswered::Reason(reason))),
        }
    }

    fn subsumes(
        &self,
        _ancestor: &seam::Coding<'_>,
        _descendant: &seam::Coding<'_>,
    ) -> Result<bool, SeamError> {
        Err(Box::new(Unanswered::Subsumption))
    }
}

/// Answers `conformsTo()` with a validation of its own against the profile
/// the package set carries.
pub(super) struct Conformance<'v, 'set> {
    validator: &'v Validator<'set>,
    depth: u8,
}

impl<'v, 'set> Conformance<'v, 'set> {
    /// Creates the profile check of `validator` for a walk `depth`
    /// `conformsTo()` calls deep.
    pub(super) const fn new(validator: &'v Validator<'set>, depth: u8) -> Self {
        Self { validator, depth }
    }
}

impl fmt::Debug for Conformance<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Conformance")
            .field("depth", &self.depth)
            .finish_non_exhaustive()
    }
}

impl seam::Profiles for Conformance<'_, '_> {
    fn conforms_to(
        &self,
        value: &Value,
        type_name: Option<&str>,
        structure: &str,
    ) -> Result<bool, SeamError> {
        if self.depth >= DEPTH {
            return Err(Box::new(Unanswered::TooDeep));
        }
        let profile = match self.validator.structure(structure, None) {
            Ok(profile) => profile,
            Err(ValidateError::Resolve(
                ResolveError::NotFound { .. } | ResolveError::Fragment { .. },
            )) => return Err(Box::new(Unanswered::Reason(Reason::ProfileNotFound))),
            Err(error) => return Err(Box::new(error)),
        };
        let mut walk = Walk::nested(self.validator, self.depth.saturating_add(1));
        if value.get("resourceType").is_some() {
            walk.resource(value, None, Some(profile))?;
        } else {
            let name = type_name
                .filter(|_| value.as_object().is_some())
                .ok_or(Unanswered::NotComplex)?;
            if profile.type_name() != name {
                return Ok(false);
            }
            walk.complex(value, name, &Cursor::new(profile, name), name)?;
        }
        Ok(walk.into_out().is_valid())
    }
}
