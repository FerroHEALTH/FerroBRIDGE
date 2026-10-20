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
//! server for the display: a found display becomes the value; a code the
//! server does not know refuses the unit unless the deployment chose the
//! code; a failed call refuses the unit; and with no server configured the
//! code stands, as the page's alternative.

use fhir_types::r4::coding::Coding;

use crate::engine::cell::Produced;
use crate::engine::fhir::FhirValue;
use crate::engine::lens::coded_text::CODEABLE_CONCEPT_CELL;
use crate::engine::lens::coded_text::CODING_CELL;
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
    /// Resolves the display of a coded text whose value the cell filled from
    /// its code.
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
        let RmValue::CodedText(ref mut coded) = produced.value else {
            return Ok(());
        };
        let system = String::from(coded.defining_code.terminology_id.name());
        let version = String::from(coded.defining_code.terminology_id.version_id());
        let code = coded.defining_code.code_string.clone();
        let coding = coding_of(view, &system, &code);
        let undisplayed = coding.is_some_and(|coding| {
            coding
                .display
                .as_ref()
                .and_then(|display| display.value.as_deref())
                .is_none_or(str::is_empty)
        });
        let from_code = undisplayed
            && produced.fallbacks.iter().any(|taken| {
                taken.field() == VALUE
                    && matches!(taken.cell(), CODING_CELL | CODEABLE_CONCEPT_CELL)
            });
        // NOTE: no specification governs this: our own design, a server addresses
        // no archetype-local code, so its code stands as the page's alternative.
        if !from_code || coded.value != code || LOCAL.contains(&system.as_str()) {
            return Ok(());
        }
        let version = (!version.is_empty()).then_some(version.as_str());
        let request = terminology::lookup_request(&system, &code, version);
        let seam = self.seams.terminology();
        let found = match seam.lookup(&request) {
            Ok(found) => found,
            // NOTE: DV_CODED_TEXT.adoc, "the coding.code can be used if a term server
            // is not an option", so with no server the code fallback stands.
            Err(TerminologyError::NotConfigured) => return Ok(()),
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
            Some(display) => {
                coded.value = String::from(display);
                produced.fallbacks.retain(|taken| taken.field() != VALUE);
                Ok(())
            }
            None if seam.display_fallback() => Ok(()),
            None => Err(EngineError::UnknownDisplay {
                mapping: String::from(mapping.name()),
                system,
                code,
            }),
        }
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
