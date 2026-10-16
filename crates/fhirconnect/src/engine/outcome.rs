// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a run produced, and the declared set of losses it took.
//!
//! An element the program cannot map refuses the unit (no specification
//! governs this: our own design), so a warning here is never "something went
//! wrong and the run carried on". Each variant of [`Loss`] is a loss the
//! specification itself declares: a mapping the direction skips (a gate, a
//! `unidirectional` marker, or a reference that resolves to nothing), a
//! `TERM_MAPPING` whose `match` is not `=` and so has no FHIR form, a
//! composition field the engine defaulted, the occurrences a `0..n` into a
//! `0..1` dropped, a one-way row a data-type cell took, and a reference whose
//! resolution belongs to the facade. Nothing else is a warning, and the
//! round-trip tests assert the set exactly. Every [`Warning`] pairs its loss
//! with the [`Location`] it happened at.

use core::fmt;

use fhir_types::codec::Value;

use crate::engine::lens::Fallback;
use crate::engine::lens::term_mapping::Withheld;

/// A produced document with the losses the run declared.
///
/// A run out of openEHR may create resources beside the one it maps, through
/// a `reference` mapping or a `hierarchy.split`; they travel as
/// [`Outcome::created`], each with the id the identity sink assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome<T> {
    value: T,
    warnings: Vec<Warning>,
    created: Vec<Value>,
}

impl<T> Outcome<T> {
    /// Wraps a produced value that took no declared loss.
    #[must_use]
    pub const fn clean(value: T) -> Self {
        Self {
            value,
            warnings: Vec::new(),
            created: Vec::new(),
        }
    }

    /// Wraps a produced value with the losses the run declared.
    #[must_use]
    pub const fn new(value: T, warnings: Vec<Warning>) -> Self {
        Self {
            value,
            warnings,
            created: Vec::new(),
        }
    }

    /// Returns this outcome with the resources the run created beside its
    /// value.
    #[must_use]
    pub fn with_created(mut self, created: Vec<Value>) -> Self {
        self.created = created;
        self
    }

    /// Returns the resources the run created beside its value, in the order
    /// it created them.
    #[must_use]
    pub fn created(&self) -> &[Value] {
        &self.created
    }

    /// Returns the produced value and the created resources, consuming the
    /// outcome.
    #[must_use]
    pub fn into_parts(self) -> (T, Vec<Value>) {
        (self.value, self.created)
    }

    /// Returns the produced value.
    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Returns the produced value, consuming the outcome.
    #[must_use]
    pub fn into_value(self) -> T {
        self.value
    }

    /// Returns the losses the run declared, in the order it took them.
    #[must_use]
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// Records one loss.
    pub fn warn(&mut self, warning: Warning) {
        self.warnings.push(warning);
    }

    /// Returns this outcome with `value` in place of the produced one.
    #[must_use]
    pub fn map<U>(self, transform: impl FnOnce(T) -> U) -> Outcome<U> {
        Outcome {
            value: transform(self.value),
            warnings: self.warnings,
            created: self.created,
        }
    }
}

/// Where a loss happened, in both trees, with no value in it.
///
/// [`Location::fhir`] is the FHIRPath of the FHIR element the loss concerns:
/// the input element going into openEHR, the output element coming out to
/// FHIR. It is written in the simple subset `OperationOutcome.issue.expression`
/// admits, "element names, repetition indicators and the default child
/// accessor"
/// (<https://hl7.org/fhir/R4/operationoutcome-definitions.html#OperationOutcome.issue.expression>),
/// rooted at the type of the resource the run reads or writes at that point
/// (a resource a `reference` mapping resolved or created included), with an
/// index on every repeating element whose instance the run knows. A choice
/// element carries no type suffix: "choice elements are labeled according to
/// the name without the '\[x\]' suffix" (<https://hl7.org/fhir/R4/fhirpath.html>,
/// §Polymorphism in FHIR). An
/// `extension(url)` step is the `extension` element at the index the url
/// matched in the document, for example `Condition.code.coding[1].code`,
/// `Observation.value` or `Patient.extension[2].value`.
///
/// [`Location::openehr`] is the archetype path of the RM node the loss
/// concerns: the `aqlPath` the Web Template gives the node, with the attribute
/// tail below it appended, for example
/// `/content[openEHR-EHR-OBSERVATION.blood_pressure.v2]/data[at0001]/events[at0006]/data[at0003]/items[at0004]/value/magnitude`.
/// It carries node ids and the template's own names, never an instance value.
///
/// Either side is `None` where the loss has no element there: a composition
/// field the engine defaulted has no FHIR element, and a mapping with no
/// `with.openehr` has no openEHR node. No specification governs the shape:
/// our own design.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Location {
    fhir: Option<String>,
    openehr: Option<String>,
}

impl Location {
    /// Returns this location with `path` as its FHIRPath.
    #[must_use]
    pub fn with_fhir(mut self, path: impl Into<String>) -> Self {
        self.fhir = Some(path.into());
        self
    }

    /// Returns this location with `path` as its openEHR path.
    #[must_use]
    pub fn with_openehr(mut self, path: impl Into<String>) -> Self {
        self.openehr = Some(path.into());
        self
    }

    /// Returns the FHIRPath of the FHIR element, when the loss has one.
    #[must_use]
    pub fn fhir(&self) -> Option<&str> {
        self.fhir.as_deref()
    }

    /// Returns the archetype path of the openEHR node, when the loss has one.
    #[must_use]
    pub fn openehr(&self) -> Option<&str> {
        self.openehr.as_deref()
    }
}

/// One loss a run declared, with where it happened.
///
/// Match on [`Warning::loss`] for what was lost and read
/// [`Warning::location`] for where. The location never carries a value, so a
/// consumer can log it without clinical content. The [`fmt::Display`] text is
/// the loss's own.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Warning {
    loss: Loss,
    location: Location,
}

impl Warning {
    /// Pairs a loss with where it happened.
    #[must_use]
    pub const fn new(loss: Loss, location: Location) -> Self {
        Self { loss, location }
    }

    /// Records the one-way row a data-type cell took at `location`.
    #[must_use]
    pub fn fallback(taken: &Fallback, location: Location) -> Self {
        Self::new(
            Loss::OneWayFallback {
                cell: String::from(taken.cell()),
                field: String::from(taken.field()),
            },
            location,
        )
    }

    /// Records the term mapping a data-type cell of `mapping` left out of the
    /// FHIR element, at `location`.
    #[must_use]
    pub fn withheld(mapping: &str, taken: &Withheld, location: Location) -> Self {
        Self::new(
            Loss::Skipped {
                mapping: String::from(mapping),
                reason: SkipReason::TermMappingMatch {
                    match_code: taken.match_code(),
                    terminology: String::from(taken.terminology()),
                    code: String::from(taken.code()),
                },
            },
            location,
        )
    }

    /// Returns what was lost.
    #[must_use]
    pub const fn loss(&self) -> &Loss {
        &self.loss
    }

    /// Returns where the loss happened.
    #[must_use]
    pub const fn location(&self) -> &Location {
        &self.location
    }

    /// Returns the loss and its location, consuming the warning.
    #[must_use]
    pub fn into_parts(self) -> (Loss, Location) {
        (self.loss, self.location)
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.loss, f)
    }
}

/// What one declared loss was.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Loss {
    /// A mapping the direction does not run.
    Skipped {
        /// The dotted name of the mapping inside its model mapping.
        mapping: String,
        /// Why the mapping did not run.
        reason: SkipReason,
    },
    /// A composition field the engine filled because no mapping did.
    ///
    /// The defaults are the ones `engine/defaults-for-fields.adoc` names, and
    /// they apply going into openEHR only.
    Defaulted {
        /// The openEHR path of the field.
        field: String,
    },
    /// The occurrences a `0..n` input lost writing into a `0..1` output.
    ///
    /// "If we transform from a `0..n` to a `0..1` field, only the last entry
    /// of `0..n` is mapped" (`recurrence/TransformingList.adoc`).
    LastOfMany {
        /// The output path that took the last occurrence.
        path: String,
        /// How many occurrences the write dropped.
        dropped: usize,
    },
    /// A data-type row the specification writes for one direction only.
    OneWayFallback {
        /// The cell that took the row.
        cell: String,
        /// The attribute the row filled.
        field: String,
    },
    /// A reference whose resolution belongs to the facade.
    ///
    /// `PARTY_IDENTIFIED.adoc` resolves a `Reference` through the demographics
    /// server and puts the resolved resource's identifier in `identifiers`.
    /// The engine makes no call, so it records the reference instead of
    /// inventing an identifier for it. The reference stays here for the
    /// facade; the warning's location is the element holding it, so the
    /// location can be logged without the identifier.
    DeferredReference {
        /// The reference string the facade resolves.
        reference: String,
    },
}

impl fmt::Display for Loss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Skipped {
                ref mapping,
                ref reason,
            } => write!(f, "the mapping {mapping} did not run: {reason}"),
            Self::Defaulted { ref field } => write!(f, "the engine filled {field}"),
            Self::LastOfMany { ref path, dropped } => write!(
                f,
                "{path} holds one value, so the write dropped {dropped} earlier occurrences"
            ),
            Self::OneWayFallback {
                ref cell,
                ref field,
            } => write!(f, "{cell} filled {field} by a rule that runs one way"),
            Self::DeferredReference { ref reference } => {
                write!(f, "{reference} is left for the facade to resolve")
            }
        }
    }
}

/// Why a mapping did not run.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkipReason {
    /// The mapping's `unidirectional` names the other direction.
    Unidirectional,
    /// A preprocessor gate of the slotted file did not admit the input.
    PreprocessorGate {
        /// The `metadata.name` of the slotted file whose gate closed.
        model: String,
    },
    /// The resource a reference points at could not be resolved.
    ///
    /// "If not possible, the engine proceeds with the mapping"
    /// (`engine/references.adoc`), so the mappings of the referenced
    /// resource do not run for it.
    UnresolvedReference {
        /// The reference as the document wrote it.
        reference: String,
    },
    /// The `Reference` carries no literal reference and no display, so it
    /// names nothing to map.
    EmptyReference,
    /// A `TERM_MAPPING` of the value has a `match` FHIR cannot carry.
    ///
    /// `match` is "hard coded to `=`, other operators not mappable to FHIR"
    /// (`types-of-mappings/data-type/TERM_MAPPING.adoc`), so the mapping
    /// wrote no `Coding` for this term; the rest of the value converted.
    TermMappingMatch {
        /// The `match` operator of the mapping: `>`, `<` or `?`.
        match_code: char,
        /// The `terminology_id` value of the mapping's target.
        terminology: String,
        /// The `code_string` of the mapping's target.
        code: String,
    },
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Unidirectional => {
                f.write_str("its unidirectional marker names the other direction")
            }
            Self::PreprocessorGate { ref model } => {
                write!(
                    f,
                    "the preprocessor gate of {model} did not admit the input"
                )
            }
            Self::UnresolvedReference { ref reference } => {
                write!(f, "the reference {reference} resolves to no resource")
            }
            Self::EmptyReference => f.write_str("the Reference names no resource"),
            Self::TermMappingMatch {
                match_code,
                ref terminology,
                ref code,
            } => write!(
                f,
                "its TERM_MAPPING `{match_code}` to {code} of {terminology} is no equivalence, \
                 and only `=` maps to a Coding"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Location;
    use super::Loss;
    use super::Outcome;
    use super::SkipReason;
    use super::Warning;
    use crate::engine::lens::Fallback;

    #[test]
    fn a_clean_outcome_declares_no_loss() {
        let outcome = Outcome::clean(7);
        assert_eq!(*outcome.value(), 7);
        assert!(
            outcome.warnings().is_empty(),
            "a clean run declares no loss"
        );
    }

    #[test]
    fn the_losses_keep_the_order_the_run_took_them() {
        let mut outcome = Outcome::clean(String::from("document"));
        outcome.warn(Warning::new(
            Loss::Skipped {
                mapping: String::from("bodySite"),
                reason: SkipReason::Unidirectional,
            },
            Location::default(),
        ));
        outcome.warn(Warning::fallback(
            &Fallback::new("DV_CODED_TEXT against CodeableConcept", "DV_TEXT.value"),
            Location::default(),
        ));
        assert_eq!(outcome.warnings().len(), 2);
        assert_eq!(
            outcome.warnings().first().map(ToString::to_string),
            Some(String::from(
                "the mapping bodySite did not run: its unidirectional marker names the other direction"
            ))
        );
    }

    #[test]
    fn a_dropped_occurrence_names_its_path_and_its_count() {
        let warning = Warning::new(
            Loss::LastOfMany {
                path: String::from("$archetype/data[at0001]/items[at0002]"),
                dropped: 2,
            },
            Location::default(),
        );
        assert!(
            warning
                .to_string()
                .contains("dropped 2 earlier occurrences"),
            "the warning states how much was lost: {warning}"
        );
    }

    #[test]
    fn a_location_carries_each_side_it_was_given() {
        let location = Location::default()
            .with_fhir("Condition.code.coding[0]")
            .with_openehr("/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]");
        assert_eq!(location.fhir(), Some("Condition.code.coding[0]"));
        assert_eq!(
            location.openehr(),
            Some("/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]")
        );
        assert_eq!(Location::default().fhir(), None);
        assert_eq!(Location::default().openehr(), None);
    }
}
