// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The imaging crosswalks (feature `imaging`).
//!
//! Art 14(1)(d) names medical imaging studies and the related imaging
//! reports, and the Xt-EHR package models each on its own, so the category
//! carries two crosswalks. [`IMAGING_REPORT`] keys on the Xt-EHR
//! `EHDSImagingReport` 1.0.0 element paths and names the elements of the
//! HL7 Europe Imaging Report 1.0.0-ballot `CompositionEuImaging` profile
//! and its `DiagnosticReportEuImaging` companion; [`IMAGING_STUDY`] keys on
//! `EHDSImagingStudy` and names the elements of `ImagingStudyEuImaging`.
//! Both name the eHealth Network ids of the *Guideline on Medical imaging
//! studies and reports* Release 1.1: §4.1, the medical imaging report data
//! set, for the report (`A.`), and §4.2, the imaging study manifest data
//! set, for the study (`B.`). The header and the section narratives are
//! written by the document assembly, so no FHIRconnect context feeds them.
//! The report's study section is assembled
//! from the `ImagingStudyEuImaging` resources the study crosswalk feeds.
//! `ImagingStudyEuImaging` has no element for the study's custodian or its
//! authoring date, so those rows name the resource and its `meta`.

use crate::category::Root;
use crate::crosswalk::Crosswalk;
use crate::crosswalk::EhnElement;
use crate::crosswalk::Populate;
use crate::crosswalk::Row;

mod report;
mod study;

/// The canonical URL of the HL7 Europe Imaging Report composition profile.
pub const IMAGING_COMPOSITION: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/CompositionEuImaging";

/// The canonical URL of the HL7 Europe Imaging Report `DiagnosticReport`
/// profile, which the composition names.
pub const IMAGING_DIAGNOSTIC_REPORT: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/DiagnosticReportEuImaging";

/// The canonical URL of the HL7 Europe Imaging Report `ImagingStudy`
/// profile.
pub const IMAGING_STUDY_PROFILE: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/ImagingStudyEuImaging";

/// The imaging report crosswalk.
pub static IMAGING_REPORT: Crosswalk<'static> = Crosswalk {
    root: Root {
        model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingReport",
        obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingReportObligations",
    },
    profile: IMAGING_COMPOSITION,
    companions: &[IMAGING_DIAGNOSTIC_REPORT],
    sliced: "Composition.section",
    ehn: EHN,
    rows: report::ROWS,
    gaps: &[],
};

/// The imaging study crosswalk.
pub static IMAGING_STUDY: Crosswalk<'static> = Crosswalk {
    root: Root {
        model: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingStudy",
        obligations: "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingStudyObligations",
    },
    profile: IMAGING_STUDY_PROFILE,
    companions: &[],
    sliced: "ImagingStudy.identifier",
    ehn: EHN,
    rows: study::ROWS,
    gaps: &[],
};

/// The eHealth Network medical imaging dataset elements the rows name, from
/// the guideline's report data set (A.1 to A.8, §4.1) and imaging study
/// manifest data set (B.1, §4.2).
const EHN: &[EhnElement] = &[
    ehn("A.1", "Report header data elements"),
    ehn("A.1.1", "Identification of the patient/subject"),
    ehn("A.1.2", "Patient/subject related contact information"),
    ehn("A.1.4", "Information recipient"),
    ehn("A.1.5", "Author"),
    ehn("A.1.6", "Legal authenticator"),
    ehn("A.1.6.4", "Legal authentication date and time"),
    ehn("A.1.7", "Result validator"),
    ehn("A.1.7.4", "Validation date and time"),
    ehn("A.1.8.0", "Document ID"),
    ehn("A.1.8.1", "Document type"),
    ehn("A.1.8.2", "Document status"),
    ehn("A.1.8.3", "Report date and time"),
    ehn("A.1.8.4", "Document title"),
    ehn("A.1.8.5", "Imaging procedure"),
    ehn("A.1.8.6", "Report custodian"),
    ehn("A.1.8.8", "Language"),
    ehn("A.1.8.11", "Accession number"),
    ehn("A.2", "Order information"),
    ehn("A.2.1", "Order ID"),
    ehn("A.2.2", "Order date and time"),
    ehn("A.3", "Order reason"),
    ehn("A.3.3", "Clinical question"),
    ehn("A.4", "Specimen information"),
    ehn("A.5", "Examination Report"),
    ehn("A.5.1", "Imaging procedure"),
    ehn("A.5.1.1", "Modality"),
    ehn("A.5.1.6", "Anatomical focus"),
    ehn("A.5.2", "Medication"),
    ehn("A.5.3", "Adverse reaction"),
    ehn("A.5.4", "Results"),
    ehn("A.5.5", "Conclusion"),
    ehn("A.5.5.1", "Impression"),
    ehn("A.5.5.2", "Coded conclusions"),
    ehn("A.5.6", "Recommendation"),
    ehn("A.5.6.1", "Description"),
    ehn("A.5.6.2", "Care plan"),
    ehn("A.7", "Comparison study"),
    ehn("A.8", "Presented form"),
    ehn("B.1", "Imaging study Manifest Dataset"),
    ehn("B.1.1", "Study instance UID"),
    ehn("B.1.2", "Description"),
    ehn("B.1.3", "Study custodian"),
    ehn("B.1.4", "List of Series"),
    ehn("B.1.4.1", "Series Description"),
    ehn("B.1.4.2", "Series Unique Identifier"),
    ehn("B.1.4.3", "Modality"),
    ehn("B.1.4.5", "Other series information"),
    ehn("B.1.4.6", "Series endpoint"),
    ehn("B.1.4.7", "List of Instances in the series"),
    ehn("B.1.4.7.1", "Instance Globally Unique Identifier"),
    ehn("B.1.4.7.2", "Instance Class Globally Unique Identifier"),
    ehn("B.1.4.7.3", "Instance Number"),
];

/// HL7 Europe Imaging Report `ServiceRequestOrderEuImaging`.
const ORDER: &str = "http://hl7.eu/fhir/imaging/StructureDefinition/ServiceRequestOrderEuImaging";
/// HL7 Europe Imaging Report `ProcedureEuImaging`.
const PROCEDURE: &str = "http://hl7.eu/fhir/imaging/StructureDefinition/ProcedureEuImaging";
/// HL7 Europe Imaging Report `ObservationFindingEuImaging`.
const FINDING: &str = "http://hl7.eu/fhir/imaging/StructureDefinition/ObservationFindingEuImaging";
/// HL7 Europe Imaging Report `ObservationRadiationDoseEuImaging`.
const RADIATION_DOSE: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/ObservationRadiationDoseEuImaging";
/// FHIR R4 `Observation`.
const OBSERVATION: &str = "http://hl7.org/fhir/StructureDefinition/Observation";
/// FHIR R4 `Condition`.
const CONDITION: &str = "http://hl7.org/fhir/StructureDefinition/Condition";
/// FHIR R4 `Device`.
const DEVICE: &str = "http://hl7.org/fhir/StructureDefinition/Device";
/// FHIR R4 `MedicationAdministration`.
const MEDICATION_ADMINISTRATION: &str =
    "http://hl7.org/fhir/StructureDefinition/MedicationAdministration";
/// FHIR R4 `AdverseEvent`.
const ADVERSE_EVENT: &str = "http://hl7.org/fhir/StructureDefinition/AdverseEvent";
/// FHIR R4 `CarePlan`.
const CARE_PLAN: &str = "http://hl7.org/fhir/StructureDefinition/CarePlan";
/// FHIR R4 `Specimen`.
const SPECIMEN: &str = "http://hl7.org/fhir/StructureDefinition/Specimen";

/// No eHealth Network counterpart.
const NONE: &[&str] = &[];
/// No FHIRconnect context: the document assembly writes the element.
const ASSEMBLED: &[&str] = &[];
/// The order a row sits in, fed by a context of the imaging order.
const ORDERED: &[&str] = &[ORDER];
/// The study a row sits in, fed by a context of the imaging study.
const STUDY: &[&str] = &[IMAGING_STUDY_PROFILE];
/// The shall obligation.
const SHALL: Option<Populate> = Some(Populate::Shall);
/// The should obligation.
const SHOULD: Option<Populate> = Some(Populate::Should);

/// Builds one eHealth Network catalogue entry.
const fn ehn(id: &'static str, name: &'static str) -> EhnElement {
    EhnElement { id, name }
}

/// Builds one row.
const fn row(
    path: &'static str,
    ehn: &'static [&'static str],
    producer: Option<Populate>,
    eps: &'static str,
    contexts: &'static [&'static str],
) -> Row {
    Row {
        path,
        ehn,
        producer,
        eps,
        contexts,
    }
}
