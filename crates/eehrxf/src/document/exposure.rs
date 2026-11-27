// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The exposure to ionising radiation an imaging report records (feature
//! `imaging`).
//!
//! Draft `EEHRxF` Annex VII Table 1 (initiative 17512, 2026-11-23) gives the
//! report a `body.exposureInformation` of a `summary` and any number of
//! `structuredDose` records, each with a `doseQuantity`, a `doseModality`, a
//! `bodySite` and a `numberOfSeries`. The Xt-EHR models hold none of them, so
//! the document writes each to the element HL7 Europe Imaging Report
//! 1.0.0-ballot has for it: the summary to the `RadiationDose` extension of
//! `CompositionEuImaging.section:procedure` ("Radiation dose summary
//! text."), and each structured dose to one `ObservationRadiationDoseEuImaging`
//! in the section's `entry:radiation-dose` slice, part of the study
//! (`partOf:study`), its body site on `Observation.bodySite` and its quantity,
//! modality and series count on the R4 `Observation.component` the profile
//! leaves open. The dose facts come from the modality, so the caller supplies
//! them; openEHR's published imaging archetypes hold none.

use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::extension::{Extension, ExtensionValue};
use fhir_types::r4::observation::{
    Observation, ObservationComponent, ObservationComponentValue, ObservationValue,
};
use fhir_types::r4::quantity::Quantity;
use fhir_types::r4::resource::Resource;

use super::section::Section;
use super::{Document, DocumentError, claiming, loinc, reference};
use crate::crosswalk::imaging::{IMAGING_COMPOSITION, IMAGING_RADIATION_DOSE};

/// The canonical URL of the HL7 Europe Imaging Report `RadiationDose`
/// extension, the radiation dose summary text of the procedure section.
pub const RADIATION_DOSE_EXTENSION: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/RadiationDose";

/// The LOINC code `ObservationRadiationDoseEuImaging.code.coding:radiation-dose`
/// fixes by pattern.
pub const RADIATION_DOSE_CODE: &str = "73569-6";

/// The section slice of `CompositionEuImaging` that holds the exposure.
const PROCEDURE: &str = "procedure";

// NOTE: draft EEHRxF Annex VII Table 1 binds no code to these elements and the profile slices
// no component, so each component code is the text of the annex element's description.
/// The component code text of `doseQuantity`.
const DOSE_QUANTITY: &str = "Ionised radiation received during the imaging investigation";
/// The component code text of `doseModality`.
const DOSE_MODALITY: &str = "Imaging modality used during the imaging investigation";
/// The component code text of `numberOfSeries`.
const NUMBER_OF_SERIES: &str = "Number of series in the study";

/// The exposure to ionising radiation during the imaging investigation,
/// Annex VII Table 1 `body.exposureInformation`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExposureInformation {
    /// `summary`: summary information about the exposure.
    pub summary: Option<String>,
    /// `structuredDose`: the structured representation of the exposure, one
    /// radiation dose entry each.
    pub doses: Vec<StructuredDose>,
}

/// One structured dose, Annex VII Table 1
/// `body.exposureInformation.structuredDose`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StructuredDose {
    /// `doseQuantity`: the ionised radiation received, in UCUM units.
    pub quantity: Option<Quantity>,
    /// `doseModality`: the imaging modality used (DICOM CID 29).
    pub modality: Option<CodeableConcept>,
    /// `bodySite`: the body part investigated.
    pub body_site: Option<CodeableConcept>,
    /// `numberOfSeries`: the number of series in the study.
    pub number_of_series: Option<i32>,
}

impl StructuredDose {
    /// The text of the dose: its quantity, modality, body site and series
    /// count, in that order, each that it holds.
    fn text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(ref quantity) = self.quantity {
            let value = quantity
                .value
                .as_ref()
                .and_then(|held| held.value.as_deref());
            let unit = quantity
                .unit
                .as_ref()
                .and_then(|held| held.value.as_deref())
                .or_else(|| {
                    quantity
                        .code
                        .as_ref()
                        .and_then(|held| held.value.as_deref())
                });
            let said: Vec<&str> = value.into_iter().chain(unit).collect();
            if !said.is_empty() {
                parts.push(said.join(" "));
            }
        }
        parts.extend(self.modality.as_ref().and_then(concept_text));
        parts.extend(self.body_site.as_ref().and_then(concept_text));
        if let Some(series) = self.number_of_series {
            parts.push(format!("{series} series"));
        }
        parts.join(", ")
    }

    /// The components of the dose: its quantity, its modality and its series
    /// count, each that it holds.
    fn components(&self) -> Vec<ObservationComponent> {
        let mut components = Vec::new();
        if let Some(ref quantity) = self.quantity {
            components.push(component(
                DOSE_QUANTITY,
                ObservationComponentValue::Quantity(Box::new(quantity.clone())),
            ));
        }
        if let Some(ref modality) = self.modality {
            components.push(component(
                DOSE_MODALITY,
                ObservationComponentValue::CodeableConcept(Box::new(modality.clone())),
            ));
        }
        if let Some(series) = self.number_of_series {
            components.push(component(
                NUMBER_OF_SERIES,
                ObservationComponentValue::Integer(series.into()),
            ));
        }
        components
    }
}

/// The text of `concept`: its text, else its first coding's display, else
/// that coding's code.
fn concept_text(concept: &CodeableConcept) -> Option<String> {
    concept
        .text
        .as_ref()
        .and_then(|held| held.value.clone())
        .or_else(|| {
            concept.coding.first().and_then(|coding| {
                coding
                    .display
                    .as_ref()
                    .and_then(|held| held.value.clone())
                    .or_else(|| coding.code.as_ref().and_then(|held| held.value.clone()))
            })
        })
}

/// The component coded by the text `code`, holding `value`.
fn component(code: &str, value: ObservationComponentValue) -> ObservationComponent {
    ObservationComponent {
        code: CodeableConcept {
            text: Some(code.into()),
            ..CodeableConcept::default()
        },
        value: Some(value),
        ..ObservationComponent::default()
    }
}

impl Document {
    /// Adds the procedure section `section` of an imaging report holding
    /// `exposure`: its summary as the section's `RadiationDose` extension and
    /// each structured dose as an `ObservationRadiationDoseEuImaging` entry
    /// about the document's patient, part of the study whose `fullUrl` is
    /// `study`.
    ///
    /// The entry's value, which the profile requires, is the summary, or the
    /// dose's own text where the exposure has none.
    ///
    /// # Errors
    ///
    /// [`DocumentError::Foreign`] when the document is no imaging report or
    /// `section` is not its procedure section, [`DocumentError::Repeated`]
    /// when the document holds that section already,
    /// [`DocumentError::Dangling`] when no patient was added or the document
    /// holds no resource under `study`, [`DocumentError::EmptyDose`] for a
    /// structured dose with no value beside an exposure with no summary, and
    /// [`DocumentError::Codec`] when an entry cannot be encoded.
    pub fn with_exposure(
        &mut self,
        mut section: Section,
        exposure: &ExposureInformation,
        study: &str,
    ) -> Result<(), DocumentError> {
        let slice = section.slot.slice;
        if self.form.composition != IMAGING_COMPOSITION || slice != PROCEDURE {
            return Err(DocumentError::Foreign { slice });
        }
        if self.sections.iter().any(|held| held.slot.slice == slice) {
            return Err(DocumentError::Repeated { slice });
        }
        let subject = self
            .patient
            .clone()
            .ok_or(DocumentError::Dangling { slice: "subject" })?;
        if !self.entries.iter().any(|(url, _)| url == study) {
            return Err(DocumentError::Dangling { slice });
        }
        let mut observations = Vec::with_capacity(exposure.doses.len());
        for dose in &exposure.doses {
            let text = exposure.summary.clone().unwrap_or_else(|| dose.text());
            if text.is_empty() {
                return Err(DocumentError::EmptyDose);
            }
            observations.push(Resource::Observation(Box::new(Observation {
                meta: Some(claiming(IMAGING_RADIATION_DOSE)),
                part_of: vec![reference(study)],
                status: "final".into(),
                code: CodeableConcept {
                    coding: vec![loinc(RADIATION_DOSE_CODE)],
                    ..CodeableConcept::default()
                },
                subject: Some(reference(&subject)),
                value: Some(ObservationValue::String(text.as_str().into())),
                body_site: dose.body_site.clone(),
                component: dose.components(),
                ..Observation::default()
            })));
        }
        for full_url in self.add(&observations)? {
            section = section.with_entry(full_url);
        }
        if let Some(ref summary) = exposure.summary {
            section = section.with_extension(Extension {
                url: RADIATION_DOSE_EXTENSION.into(),
                value: Some(ExtensionValue::String(summary.as_str().into())),
                ..Extension::default()
            });
        }
        self.with_section(section)
    }
}
