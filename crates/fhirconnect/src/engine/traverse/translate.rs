// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The translation of a coded value through the concept map its mapping
//! names.
//!
//! The code the run reads is the source of the translation in both
//! directions: going into openEHR the FHIR coding the resource carries, coming
//! out the coding the data-type cell read from the composition. Each distinct
//! code is translated once per run, in the order the walk reaches it, so every
//! call precedes the composition build and a terminology fault refuses the
//! unit before anything is built.

use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;

use crate::engine::fhir::FhirValue;
use crate::engine::outcome::Loss;
use crate::engine::outcome::SkipReason;
use crate::engine::outcome::Warning;
use crate::engine::terminology;
use crate::engine::terminology::TerminologyError;
use crate::resolve::program::mapping::Mapping;
use crate::resolve::program::target::FhirTarget;
use crate::resolve::program::target::OpenehrTarget;
use crate::tree::Occurrence;
use crate::tree::element::Move;
use crate::tree::element::Table;

use crate::engine::traverse::Run;
use crate::engine::traverse::error::EngineError;

/// The output element a translated value is written to.
#[derive(Debug, Clone, Copy)]
pub(super) struct Output<'o> {
    /// Whether the output requires the element.
    pub(super) required: bool,
    /// The element: an openEHR `aqlPath` or a FHIR element path.
    pub(super) element: &'o str,
    /// The FHIR occurrence the loss is located at.
    pub(super) occurrence: &'o Occurrence,
}

impl<'o> Output<'o> {
    /// Returns the openEHR node `target` names as the output, required when
    /// the template's lower occurrence bound is at least one.
    pub(super) fn openehr(target: &'o OpenehrTarget, occurrence: &'o Occurrence) -> Self {
        let node = target.node();
        Self {
            required: node.min().is_some_and(|min| min >= 1),
            element: node.aql_path().as_str(),
            occurrence,
        }
    }

    /// Returns the FHIR element `target` ends on as the output.
    pub(super) fn fhir(target: &'o FhirTarget, occurrence: &'o Occurrence) -> Self {
        Self {
            required: required(target),
            element: target.resolved().leaf(),
            occurrence,
        }
    }
}

/// One distinct code a run translated: the concept map, the system, the
/// system version and the code.
pub(super) type TranslationKey = (String, String, Option<String>, String);

impl<T: Table + ?Sized> Run<'_, T> {
    /// Returns `view` translated through the concept map `mapping` names,
    /// `None` when the map translates it to no equivalent concept and the
    /// `output` element is optional.
    ///
    /// A mapping that names no concept map, and a value that carries no code,
    /// pass unchanged: the specification transforms "all codes contained in
    /// the conceptmap"
    /// (`types-of-mappings/concept-type/manual.adoc`, §`ConceptMaps`).
    pub(super) fn translated(
        &mut self,
        mapping: &Mapping,
        view: FhirValue,
        output: Output<'_>,
    ) -> Result<Option<FhirValue>, EngineError> {
        let Some(url) = mapping.conceptmap() else {
            return Ok(Some(view));
        };
        let refuse = |source: TerminologyError| EngineError::Terminology {
            mapping: String::from(mapping.name()),
            conceptmap: String::from(url),
            source: Box::new(source),
        };
        let translated = match view {
            FhirValue::Coding(coding) => {
                self.concept_of(url, &coding)
                    .map_err(refuse)?
                    .map(|concept| {
                        FhirValue::Coding(Box::new(terminology::translated(&coding, &concept)))
                    })
            }
            FhirValue::CodeableConcept(concept) if concept.coding.is_empty() => {
                return Ok(Some(FhirValue::CodeableConcept(concept)));
            }
            FhirValue::CodeableConcept(concept) => self
                .translate_concept(url, &concept)
                .map_err(refuse)?
                .map(|translated| FhirValue::CodeableConcept(Box::new(translated))),
            other @ (FhirValue::String(_)
            | FhirValue::Date(_)
            | FhirValue::DateTime(_)
            | FhirValue::Boolean(_)
            | FhirValue::Period(_)
            | FhirValue::Reference(_)
            | FhirValue::Identifier(_)
            | FhirValue::Quantity(_)) => return Ok(Some(other)),
        };
        if translated.is_none() {
            self.untranslated(mapping, url, output)?;
        }
        Ok(translated)
    }

    /// Returns the concept `source` translates to through `url`, as the
    /// server named it, asking the seam once per distinct code.
    fn concept_of(
        &mut self,
        url: &str,
        source: &Coding,
    ) -> Result<Option<Coding>, TerminologyError> {
        let request = terminology::request(url, source)?;
        let key: TranslationKey = (
            String::from(url),
            request
                .system
                .as_ref()
                .and_then(|uri| uri.value.clone())
                .unwrap_or_default(),
            request
                .version
                .as_ref()
                .and_then(|version| version.value.clone()),
            request
                .code
                .as_ref()
                .and_then(|code| code.value.clone())
                .unwrap_or_default(),
        );
        if let Some(held) = self.translations.get(&key) {
            return Ok(held.clone());
        }
        let response = self.seams.terminology().translate(&request)?;
        let concept = terminology::accepted(&response)?;
        self.translations.insert(key, concept.clone());
        Ok(concept)
    }

    /// Returns the concept `source` translates to through `url`.
    ///
    /// Every coding is translated in order and the translated ones are kept,
    /// once each; the codings the map does not translate are dropped, because
    /// writing them would assert a code of another system. The `text` is the
    /// display of the first translated concept, and the source text only when
    /// that concept names none. No specification governs how a
    /// `CodeableConcept` is translated coding by coding: our own design.
    fn translate_concept(
        &mut self,
        url: &str,
        source: &CodeableConcept,
    ) -> Result<Option<CodeableConcept>, TerminologyError> {
        let mut codings: Vec<Coding> = Vec::new();
        let mut first: Option<Coding> = None;
        for coding in &source.coding {
            let Some(concept) = self.concept_of(url, coding)? else {
                continue;
            };
            let translated = terminology::translated(coding, &concept);
            let known = codings
                .iter()
                .any(|held| held.system == translated.system && held.code == translated.code);
            if !known {
                codings.push(translated);
            }
            first.get_or_insert(concept);
        }
        let Some(first) = first else {
            return Ok(None);
        };
        let text = terminology::translated(
            &Coding {
                display: source.text.clone(),
                ..Coding::default()
            },
            &first,
        )
        .display;
        Ok(Some(CodeableConcept {
            coding: codings,
            text,
            ..CodeableConcept::default()
        }))
    }
}

impl<T: Table + ?Sized> Run<'_, T> {
    /// Records a code the concept map `conceptmap` of `mapping` did not
    /// translate, writing nothing at the output element, or refuses when the
    /// output requires it.
    ///
    /// The source code is never written under the target in its place: an
    /// optional element is left out and the loss declared, and a required one
    /// refuses the unit as a required child with no value does ("the mapping
    /// should fail if the child is not provided", `engine/Fail.adoc`).
    fn untranslated(
        &mut self,
        mapping: &Mapping,
        conceptmap: &str,
        output: Output<'_>,
    ) -> Result<(), EngineError> {
        let conceptmap = String::from(conceptmap);
        if output.required {
            return Err(EngineError::Untranslated {
                mapping: String::from(mapping.name()),
                conceptmap,
                element: String::from(output.element),
            });
        }
        let location = self.locate(mapping, output.occurrence);
        self.warnings.push(Warning::new(
            Loss::Skipped {
                mapping: String::from(mapping.name()),
                reason: SkipReason::Untranslated { conceptmap },
            },
            location,
        ));
        Ok(())
    }
}

/// Returns whether the FHIR element `target` ends on is required.
///
/// The minimum cardinality of the last element the path names is the
/// element table's (<https://hl7.org/fhir/R4/elementdefinition.html>,
/// `ElementDefinition.min`).
fn required(target: &FhirTarget) -> bool {
    target
        .resolved()
        .moves()
        .iter()
        .rev()
        .find_map(|step| match *step {
            Move::Member(ref field)
            | Move::Choice { ref field, .. }
            | Move::Extension { ref field, .. } => Some(field.min() >= 1),
            Move::Ordinal(_) | Move::Index(_) | Move::Predicate(_) | Move::Resolve { .. } => None,
        })
        .unwrap_or(false)
}
