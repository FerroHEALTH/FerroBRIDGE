// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The profiles a type of an element names (`ElementDefinition.type.profile`):
//! "the content must conform to at least one of them"
//! (<https://hl7.org/fhir/R4/elementdefinition-definitions.html#ElementDefinition.type.profile>).

use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::r4::element_definition::ElementDefinition;

use crate::canonical::Canonical;
use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::report::Collector;
use crate::report::Constraint;
use crate::report::Reason;
use crate::set::ResolveError;
use crate::validate::ValidateError;
use crate::validate::structure::Structure;
use crate::validate::walk::Cursor;
use crate::validate::walk::Held;
use crate::validate::walk::Occurrence;
use crate::validate::walk::Walk;

/// The content validated against a profile.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Subject<'a, 'i> {
    /// A datatype value of the type `name`.
    Complex {
        /// The value.
        value: &'i Value,
        /// The type name.
        name: &'a str,
    },
    /// A resource.
    Resource {
        /// The resource.
        value: &'i Value,
    },
    /// A primitive: its value and its `_` sibling.
    Primitive {
        /// The occurrence.
        occurrence: &'a Occurrence<'i>,
        /// The primitive type.
        code: &'a str,
    },
}

/// The outcome of validating content against one profile.
#[derive(Debug)]
pub(crate) enum Conformance {
    /// The findings of the validation, which passes when none is an error.
    Checked(Collector),
    /// The profile could not be validated against, for the reason given.
    Unknown(Reason),
}

/// The profiles the types of `element` coded `code` name.
pub(crate) fn profiles_for(element: Option<&ElementDefinition>, code: &str) -> Vec<String> {
    element
        .into_iter()
        .flat_map(|element| &element.r#type)
        .filter(|kind| kind.code.value.as_deref() == Some(code))
        .flat_map(|kind| &kind.profile)
        .filter_map(|profile| profile.value.clone())
        .collect()
}

/// The profiles the types of `element` name for a resource of type `kind`:
/// the types coded `kind`, `Resource` or `DomainResource`.
pub(crate) fn resource_profiles(element: Option<&ElementDefinition>, kind: &str) -> Vec<String> {
    element
        .into_iter()
        .flat_map(|element| &element.r#type)
        .filter(|entry| {
            matches!(entry.code.value.as_deref(), Some(code) if code == kind || code == "Resource" || code == "DomainResource")
        })
        .flat_map(|entry| &entry.profile)
        .filter_map(|profile| profile.value.clone())
        .collect()
}

impl<'i> Walk<'_, '_, 'i> {
    /// Validates `subject` against `profile`, as the definition `from` names
    /// it, in a walk of its own.
    pub(crate) fn conformance(
        &self,
        subject: Subject<'_, 'i>,
        (profile, from): (&str, &Structure),
        location: &str,
    ) -> Result<Conformance, ValidateError> {
        let structure = match self.validator().structure(profile, Some(from)) {
            Ok(structure) => structure,
            Err(ValidateError::Resolve(
                ResolveError::NotFound { .. } | ResolveError::Fragment { .. },
            )) => {
                return Ok(Conformance::Unknown(Reason::ProfileNotFound));
            }
            Err(error) => return Err(error),
        };
        let mut fork = self.fork();
        let name = match subject {
            Subject::Resource { value } => {
                fork.resource(value, Some(location), Some(structure))?;
                return Ok(Conformance::Checked(fork.into_out()));
            }
            Subject::Complex { name, .. } | Subject::Primitive { code: name, .. } => name,
        };
        if structure.type_name() != name {
            fork.out().issue(
                Severity::Error,
                IssueType::Structure,
                MessageId::ProfileTypeMismatch,
                location,
                format!(
                    "the profile {} constrains {}, not {name}",
                    structure.url(),
                    structure.type_name()
                ),
            );
            return Ok(Conformance::Checked(fork.into_out()));
        }
        let root = Cursor::new(Arc::clone(&structure), name);
        match subject {
            Subject::Complex { value, name } => fork.complex(value, name, &root, location)?,
            Subject::Primitive { occurrence, code } => {
                if let Some(element) = structure.element(code) {
                    fork.checks(&structure, code, element, occurrence)?;
                }
                if let Some(element) = occurrence.element {
                    fork.complex(element, "Element", &root, location)?;
                }
            }
            Subject::Resource { .. } => {}
        }
        Ok(Conformance::Checked(fork.into_out()))
    }

    /// Validates `subject` against each of `profiles`, passing when it
    /// conforms to one, and answers whether the content still needs the walk
    /// against its base definition.
    ///
    /// The findings of the profile it conforms to join the report; with one
    /// profile its findings join whether it passes or not. A profile the set
    /// does not carry is listed as unevaluated.
    pub(crate) fn type_profiles(
        &mut self,
        cursor: &Cursor,
        target: Held<'_>,
        subject: Subject<'_, 'i>,
        profiles: &[String],
        location: &str,
    ) -> Result<bool, ValidateError> {
        let mut checked = Vec::new();
        for profile in profiles {
            match self.conformance(subject, (profile, &cursor.structure), location)? {
                Conformance::Checked(found) => checked.push((profile, found)),
                Conformance::Unknown(reason) => self.out().unevaluated(
                    cursor.structure.url(),
                    target.id,
                    Constraint::TypeProfile {
                        profiles: vec![profile.clone()],
                    },
                    location,
                    reason,
                ),
            }
        }
        if let Some(index) = checked.iter().position(|(_, found)| found.is_valid()) {
            let (_, found) = checked.swap_remove(index);
            self.out().absorb(found);
            return Ok(false);
        }
        if let ([(_, _)], [_]) = (checked.as_slice(), profiles) {
            if let Some((_, found)) = checked.pop() {
                self.out().absorb(found);
            }
            return Ok(false);
        }
        if checked.is_empty() {
            return Ok(true);
        }
        let all = profiles.join(", ");
        for (profile, found) in checked {
            let first = found.first_error().map_or_else(String::new, |issue| {
                format!(": {} at {}", issue.message, issue.expression)
            });
            self.out().issue(
                Severity::Error,
                IssueType::Structure,
                MessageId::TypeProfileMismatch,
                location,
                format!("the content conforms to none of {all}; against {profile}{first}"),
            );
        }
        Ok(true)
    }

    /// Validates an extension against the profiles its element names when
    /// its url is none of them; an extension whose url is one of them is
    /// walked against that definition already.
    pub(crate) fn extension_profiles(
        &mut self,
        cursor: &Cursor,
        target: Held<'_>,
        value: &'i Value,
        location: &str,
    ) -> Result<(), ValidateError> {
        let profiles = profiles_for(target.element, "Extension");
        let url = value.get("url").and_then(Value::as_str);
        let named = url.is_some_and(|url| {
            profiles
                .iter()
                .any(|profile| Canonical::parse(profile).url() == url)
        });
        if !profiles.is_empty() && !named {
            let subject = Subject::Complex {
                value,
                name: "Extension",
            };
            self.type_profiles(cursor, target, subject, &profiles, location)?;
        }
        Ok(())
    }

    /// Validates a primitive against the profiles its element names: the
    /// constraints of the profile's root element on the value, and its
    /// elements on the `_` sibling.
    pub(crate) fn primitive_profiles(
        &mut self,
        cursor: &Cursor,
        target: Held<'_>,
        occurrence: &Occurrence<'i>,
        code: &str,
    ) -> Result<(), ValidateError> {
        let profiles = profiles_for(target.element, code);
        if !profiles.is_empty() {
            let subject = Subject::Primitive { occurrence, code };
            self.type_profiles(cursor, target, subject, &profiles, &occurrence.location)?;
        }
        Ok(())
    }
}
