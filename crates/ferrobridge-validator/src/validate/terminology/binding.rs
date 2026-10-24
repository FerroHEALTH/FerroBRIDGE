// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology binding of an element definition held to one value: its
//! strength, its maximum value set, and the displays of its codings
//! (<https://hl7.org/fhir/R4/terminologies.html#strength>).

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::definition::ElementDefinitionRead;
use fhir_types::schema::Schemas;

use ferrobridge_term::concept::Concept;
use ferrobridge_term::outcome::Validation;

use crate::canonical::Canonical;
use crate::release::Release;
use crate::report::Constraint;
use crate::report::Reason;
use crate::set::PackageId;
use crate::terminology::PendingBinding;
use crate::terminology::Strength;
use crate::terminology::binding_issue;
use crate::terminology::display_issue;
use crate::validate::Element;
use crate::validate::ValidateError;
use crate::validate::structure::Structure;
use crate::validate::terminology::Coded;
use crate::validate::terminology::Coding;
use crate::validate::terminology::Form;
use crate::validate::terminology::Membership;
use crate::validate::walk::Walk;

/// The R4 types a `Quantity` binding constrains the unit of
/// (datatypes.html#Quantity); a type the element table makes a
/// specialization of `Quantity` is one too.
const QUANTITIES: [&str; 7] = [
    "Quantity",
    "SimpleQuantity",
    "Age",
    "Count",
    "Distance",
    "Duration",
    "MoneyQuantity",
];

/// A value held to a binding: a primitive's text or a datatype's object.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Bound<'a> {
    /// The JSON string of a primitive.
    Text(&'a str),
    /// The JSON object of a datatype.
    Object(&'a Object),
}

impl<'a> Bound<'a> {
    /// Returns `value` as a bound value, `None` for any other JSON type.
    pub(crate) const fn of(value: &'a Value) -> Option<Self> {
        match value {
            Value::String(text) => Some(Self::Text(text.as_str())),
            Value::Object(object) => Some(Self::Object(object)),
            _ => None,
        }
    }
}

/// How a value of one type reads as a coded value.
enum Reading<'a> {
    /// The coded value.
    Coded(Coded<'a>),
    /// The JSON shape is not the type's, which the walk reports elsewhere.
    Skip,
    /// The type is none a binding is checked on.
    Unsupported,
}

/// Reads `value` of the type `code` as a coded value: a `code`, `string`
/// or `uri` by its text (terminologies.html §4.1.6.1), a `Coding`, a
/// `CodeableConcept`, or a quantity by its unit, its specializations read
/// from the element table `schemas`.
fn reading<'a>(schemas: &Schemas, code: &str, value: Bound<'a>) -> Reading<'a> {
    let quantity = |kind: &str| QUANTITIES.contains(&kind) || schemas.specializes(kind, "Quantity");
    match (code, value) {
        ("code" | "string" | "uri", Bound::Text(text)) => Reading::Coded(Coded::code(text)),
        ("Coding", Bound::Object(object)) => Reading::Coded(Coded::coding(object)),
        ("CodeableConcept", Bound::Object(object)) => Reading::Coded(Coded::concept(object)),
        (kind, Bound::Object(object)) if quantity(kind) => Reading::Coded(Coded::coding(object)),
        ("code" | "string" | "uri" | "Coding" | "CodeableConcept", _) => Reading::Skip,
        (kind, _) if quantity(kind) => Reading::Skip,
        _ => Reading::Unsupported,
    }
}

impl<R: Release> Walk<'_, '_, '_, R> {
    /// Holds `value`, of the type `code`, to the binding `element` declares.
    ///
    /// # Errors
    ///
    /// Returns [`ValidateError::Resolve`] when a value set or code system the
    /// set carries cannot be read.
    pub(crate) fn binding(
        &mut self,
        structure: &Structure<R>,
        (id, element): (&str, &Element<R>),
        code: &str,
        value: Bound<'_>,
        location: &str,
    ) -> Result<(), ValidateError> {
        let Some(binding) = element.binding() else {
            return Ok(());
        };
        let text = binding.strength.unwrap_or_default().to_owned();
        let value_set = binding.value_set.map(str::to_owned);
        let constraint = Constraint::Binding {
            strength: text.clone(),
            value_set: value_set.clone(),
        };
        // NOTE: terminologies.html §4.1.5.3.2: an example binding is no conformance requirement,
        // so its value set is not checked and not listed.
        let strength = match text.as_str() {
            "required" => Some(Strength::Required),
            "extensible" => Some(Strength::Extensible),
            "preferred" => Some(Strength::Preferred),
            "example" => None,
            other => {
                let reason = Reason::BindingStrength(other.to_owned());
                self.out()
                    .unevaluated(structure.url(), id, constraint, location, reason);
                return Ok(());
            }
        };
        let max = binding.max_value_set.map(str::to_owned);
        let coded = match reading(R::schemas(), code, value) {
            Reading::Coded(coded) => coded,
            Reading::Skip => return Ok(()),
            Reading::Unsupported => {
                let reason = Reason::BindingType(code.to_owned());
                if strength.is_some() {
                    self.out().unevaluated(
                        structure.url(),
                        id,
                        constraint.clone(),
                        location,
                        reason.clone(),
                    );
                }
                if let Some(max) = max {
                    let constraint = Constraint::MaxValueSet { value_set: max };
                    self.out()
                        .unevaluated(structure.url(), id, constraint, location, reason);
                }
                return Ok(());
            }
        };
        if let Some(strength) = strength {
            match &value_set {
                Some(value_set) => {
                    let check = (constraint, strength, value_set.as_str());
                    self.bound(structure, id, check, &coded, location)?;
                }
                None => self.out().unevaluated(
                    structure.url(),
                    id,
                    constraint,
                    location,
                    Reason::BindingWithoutValueSet,
                ),
            }
        }
        if let Some(max) = max {
            let constraint = Constraint::MaxValueSet {
                value_set: max.clone(),
            };
            self.bound(
                structure,
                id,
                (constraint, Strength::Maximum, &max),
                &coded,
                location,
            )?;
        }
        self.displays(structure, &coded, location)
    }

    /// Holds `coded` to one value set at one strength, locally or as a
    /// check pending for a terminology server.
    fn bound(
        &mut self,
        structure: &Structure<R>,
        id: &str,
        (constraint, strength, value_set): (Constraint, Strength, &str),
        coded: &Coded<'_>,
        location: &str,
    ) -> Result<(), ValidateError> {
        let scope = structure.origin();
        let reason = match self.validator().holds(value_set, scope, coded)? {
            Membership::Member => return Ok(()),
            Membership::NotMember => {
                self.out()
                    .push(binding_issue(strength, id, value_set, location));
                return Ok(());
            }
            Membership::Unknown(reason) => reason,
        };
        let Some(validation) = self.question(value_set, scope, coded)? else {
            self.out().unevaluated(
                structure.url(),
                id,
                constraint,
                location,
                Reason::NoCodeSystem,
            );
            return Ok(());
        };
        let display = match (coded.form, coded.codes.first(), strength) {
            (
                Form::Coding,
                Some(code),
                Strength::Required | Strength::Extensible | Strength::Preferred,
            ) => match (code.system, code.display) {
                (Some(system), Some(display))
                    if self.validator().code_index(system, scope)?.is_err() =>
                {
                    Some(display.to_owned())
                }
                _ => None,
            },
            _ => None,
        };
        let pending = PendingBinding {
            key: (structure.url().to_owned(), id.to_owned(), constraint),
            location: location.to_owned(),
            strength,
            value_set: value_set.to_owned(),
            validation,
            display,
        };
        self.out().pending(pending, reason);
        Ok(())
    }

    /// Returns the `$validate-code` question for `coded`, `None` for a bare
    /// code when the value set names no single system it could come from.
    fn question(
        &self,
        value_set: &str,
        scope: PackageId,
        coded: &Coded<'_>,
    ) -> Result<Option<Validation>, ValidateError> {
        let canonical = Canonical::parse(value_set);
        let coded = match coded.form {
            Form::Code => {
                let Some(system) = self.validator().single_system(value_set, scope)? else {
                    return Ok(None);
                };
                let Some(code) = coded.codes.first().and_then(|code| code.code) else {
                    return Ok(None);
                };
                ferrobridge_term::outcome::Coded::Code {
                    system,
                    code: code.to_owned(),
                    version: None,
                    display: None,
                }
            }
            Form::Coding => match coded.codes.first() {
                Some(code) => ferrobridge_term::outcome::Coded::Coding(concept(code)),
                None => return Ok(None),
            },
            Form::CodeableConcept => ferrobridge_term::outcome::Coded::CodeableConcept {
                coding: coded.codes.iter().map(concept).collect(),
                text: coded.text.map(str::to_owned),
            },
        };
        Ok(Some(Validation {
            value_set_url: canonical.url().to_owned(),
            value_set_version: canonical.version().map(str::to_owned),
            date: None,
            coded,
        }))
    }

    /// Warns of every coding whose display is none the code system, carried
    /// by the set, gives its code.
    fn displays(
        &mut self,
        structure: &Structure<R>,
        coded: &Coded<'_>,
        location: &str,
    ) -> Result<(), ValidateError> {
        if coded.form == Form::Code {
            return Ok(());
        }
        for code in &coded.codes {
            let (Some(system), Some(text), Some(display)) = (code.system, code.code, code.display)
            else {
                continue;
            };
            let at = match coded.form {
                Form::CodeableConcept => format!("{location}.coding[{}]", code.position),
                _ => location.to_owned(),
            };
            if !self.first_display(&at) {
                continue;
            }
            let Ok(index) = self.validator().code_index(system, structure.origin())? else {
                continue;
            };
            let Some(concept) = index.concept(text) else {
                continue;
            };
            // NOTE: no specification governs this comparison: our own design; a display matches
            // the concept's display or one of its designations exactly (codesystem.html §4.8.9).
            let known = concept.display.as_deref() == Some(display)
                || concept.designations.iter().any(|held| held == display);
            if !known {
                self.out()
                    .push(display_issue(&at, system, concept.display.as_deref()));
            }
        }
        Ok(())
    }
}

/// The neutral `Coding` of one code.
fn concept(code: &Coding<'_>) -> Concept {
    Concept {
        system: code.system.map(str::to_owned),
        version: code.version.map(str::to_owned),
        code: code.code.map(str::to_owned),
        display: code.display.map(str::to_owned),
    }
}
