// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The display of a code a FHIR coding carries none for, resolved through
//! the terminology seam going into openEHR.
//!
//! "We recommend to resolve the code display using a terminology server when
//! mapping to openEHR, since the value is mandatory in openEHR. Alternatively
//! the coding.code can be used if a term server is not an option"
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_CODED_TEXT.adoc`).
//! So a `DV_CODED_TEXT` whose value the cell filled from the code asks the
//! server for the display, and so does a `DV_TEXT` whose rubric fell back to
//! a `CodeableConcept`'s first code: a found display becomes the value; a code the
//! server does not know refuses the unit unless the deployment chose the
//! code; a failed call refuses the unit; and with no server configured the
//! code stands, as the page's alternative.

use fhir_types::r4::coding::Coding;

use crate::engine::cell::Produced;
use crate::engine::fhir::FhirValue;
use crate::engine::lens::coded_text::CODEABLE_CONCEPT_CELL;
use crate::engine::lens::coded_text::CODING_CELL;
use crate::engine::lens::text::CODEABLE_CONCEPT_CELL as TEXT_CODEABLE_CONCEPT_CELL;
use crate::engine::rm::RmValue;
use crate::engine::terminology;
use crate::engine::terminology::TerminologyError;
use crate::resolve::program::mapping::Mapping;
use crate::tree::element::Table;

use crate::engine::traverse::Run;
use crate::engine::traverse::error::EngineError;

/// The attribute the code fallback of a coded text fills.
const VALUE: &str = "DV_TEXT.value";

/// The terminologies no terminology server addresses: the archetype's own
/// codes and the openEHR terminology.
const LOCAL: [&str; 2] = ["local", "openehr"];

impl<T: Table + ?Sized> Run<'_, T> {
    /// Resolves the display of a coded text, or of a text, whose value the
    /// cell filled from a code.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::DisplayLookup`] when the server failed, and
    /// [`EngineError::UnknownDisplay`] when it does not know the code and the
    /// deployment did not choose the code as the display.
    pub(super) fn resolve_display(
        &self,
        mapping: &Mapping,
        view: &FhirValue,
        produced: &mut Produced<RmValue>,
    ) -> Result<(), EngineError> {
        let found = match produced.value {
            RmValue::CodedText(ref mut coded) => {
                let system = String::from(coded.defining_code.terminology_id.name());
                let version = String::from(coded.defining_code.terminology_id.version_id());
                let code = coded.defining_code.code_string.clone();
                let from_code = coding_of(view, &system, &code).is_some_and(undisplayed)
                    && filled_from_code(produced, &[CODING_CELL, CODEABLE_CONCEPT_CELL]);
                // NOTE: no specification governs this: our own design, a server addresses
                // no archetype-local code, so its code stands as the page's alternative.
                if !from_code || coded.value != code || LOCAL.contains(&system.as_str()) {
                    return Ok(());
                }
                let version = (!version.is_empty()).then_some(version.as_str());
                let display = self.looked_up(mapping, system, code, version)?;
                if let Some(ref display) = display {
                    coded.value.clone_from(display);
                }
                display
            }
            // NOTE: text.rs `rubric_of` reads DV_TEXT.adoc by DV_CODED_TEXT.adoc, whose
            // code fallback comes with the advice to resolve the display through a server.
            RmValue::Text(ref mut text) => {
                let Some(coding) = first_coding(view) else {
                    return Ok(());
                };
                let (Some(system), Some(code)) = (
                    coding.system.as_ref().and_then(|uri| uri.value.as_deref()),
                    coding.code.as_ref().and_then(|held| held.value.as_deref()),
                ) else {
                    return Ok(());
                };
                let from_code = undisplayed(&coding)
                    && filled_from_code(produced, &[TEXT_CODEABLE_CONCEPT_CELL]);
                if !from_code || text.value != code {
                    return Ok(());
                }
                let version = coding
                    .version
                    .as_ref()
                    .and_then(|held| held.value.as_deref())
                    .filter(|held| !held.is_empty());
                let display =
                    self.looked_up(mapping, String::from(system), String::from(code), version)?;
                if let Some(ref display) = display {
                    text.value.clone_from(display);
                }
                display
            }
            _ => return Ok(()),
        };
        if found.is_some() {
            produced.fallbacks.retain(|taken| taken.field() != VALUE);
        }
        Ok(())
    }

    /// Asks the terminology seam for the display of `code` in `system`.
    ///
    /// Returns `None` when the code stands as the value: no server is
    /// configured, or the server does not know the code and the deployment
    /// chose the code as the display.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::DisplayLookup`] when the server failed, and
    /// [`EngineError::UnknownDisplay`] when it does not know the code and the
    /// deployment did not choose the code as the display.
    fn looked_up(
        &self,
        mapping: &Mapping,
        system: String,
        code: String,
        version: Option<&str>,
    ) -> Result<Option<String>, EngineError> {
        let request = terminology::lookup_request(&system, &code, version);
        let seam = self.seams.terminology();
        let found = match seam.lookup(&request) {
            Ok(found) => found,
            // NOTE: DV_CODED_TEXT.adoc, "the coding.code can be used if a term server
            // is not an option", so with no server the code fallback stands.
            Err(TerminologyError::NotConfigured) => return Ok(None),
            Err(source) => {
                return Err(EngineError::DisplayLookup {
                    mapping: String::from(mapping.name()),
                    system,
                    code,
                    source: Box::new(source),
                });
            }
        };
        let display = found
            .as_ref()
            .and_then(|answer| answer.display.value.as_deref())
            .filter(|display| !display.is_empty());
        match display {
            Some(display) => Ok(Some(String::from(display))),
            None if seam.display_fallback() => Ok(None),
            None => Err(EngineError::UnknownDisplay {
                mapping: String::from(mapping.name()),
                system,
                code,
            }),
        }
    }
}

/// Whether `coding` carries no display.
fn undisplayed(coding: &&Coding) -> bool {
    coding
        .display
        .as_ref()
        .and_then(|display| display.value.as_deref())
        .is_none_or(str::is_empty)
}

/// Whether one of `cells` filled the value from a code.
fn filled_from_code(produced: &Produced<RmValue>, cells: &[&str]) -> bool {
    produced
        .fallbacks
        .iter()
        .any(|taken| taken.field() == VALUE && cells.contains(&taken.cell()))
}

/// Returns the first coding of a `CodeableConcept` view, the one a text's
/// rubric falls back to.
fn first_coding(view: &FhirValue) -> Option<&Coding> {
    match *view {
        FhirValue::CodeableConcept(ref concept) => concept.coding.first(),
        _ => None,
    }
}

/// Returns the coding in `view` that names `code` in `system`.
fn coding_of<'view>(view: &'view FhirValue, system: &str, code: &str) -> Option<&'view Coding> {
    let names = |coding: &&Coding| {
        coding.system.as_ref().and_then(|uri| uri.value.as_deref()) == Some(system)
            && coding.code.as_ref().and_then(|held| held.value.as_deref()) == Some(code)
    };
    match *view {
        FhirValue::Coding(ref coding) => Some(coding.as_ref()).filter(names),
        FhirValue::CodeableConcept(ref concept) => concept.coding.iter().find(names),
        _ => None,
    }
}
