// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `Composition.section` slice that holds the narrative of each data
//! group a draft `EEHRxF` annex declares.
//!
//! Draft `EEHRxF` Art 3(1)(b) (initiative 17512, 2026-11-23) exchanges a
//! document as FHIR narrative organised by data group. Each row names a data
//! group (annex, Xt-EHR model, draft element) and the section slice of the
//! category's HL7 Europe `Composition` profile whose narrative carries it, or
//! `None` where the profile holds the group outside any section slice. The
//! slices follow the category crosswalks. No specification publishes this
//! map: our own design, held to the pinned profile snapshots by
//! [`super::every_group_slice_is_a_section_slice_of_its_profile`] and to the
//! annexes by [`super::every_data_group_has_a_slice_row`].

use eehrxf::draft::Annex;

/// One data group and the section slice that holds its narrative.
pub(super) struct Group {
    /// The annex of the data group.
    pub(super) annex: Annex,
    /// The Xt-EHR model the draft crosswalk sets the group in.
    pub(super) model: &'static str,
    /// The draft element of the group.
    pub(super) element: &'static str,
    /// The section slice, or `None` where no slice holds the group.
    pub(super) slice: Option<&'static str>,
}

/// The row of the data group `element` of `model` in `annex`, narrated in
/// `slice`.
const fn group(
    annex: Annex,
    model: &'static str,
    element: &'static str,
    slice: Option<&'static str>,
) -> Group {
    Group {
        annex,
        model,
        element,
        slice,
    }
}

/// The patient summary model.
const PS: &str = "EHDSPatientSummary";

/// The imaging report model.
const IR: &str = "EHDSImagingReport";

/// The laboratory report model.
const LR: &str = "EHDSLaboratoryReport";

/// The discharge report model.
const DR: &str = "EHDSDischargeReport";

/// The rows, in annex order.
pub(super) const GROUPS: &[Group] = &[
    group(Annex::Iv, PS, "alerts", Some("sectionAlert")),
    group(
        Annex::Iv,
        PS,
        "allergiesAndIntolerances",
        Some("sectionAllergies"),
    ),
    group(Annex::Iv, PS, "problems", Some("sectionProblems")),
    group(
        Annex::Iv,
        PS,
        "medicationSummary",
        Some("sectionMedications"),
    ),
    group(
        Annex::Iv,
        PS,
        "medicalDevicesAndImplants",
        Some("sectionMedicalDevices"),
    ),
    group(Annex::Iv, PS, "procedures", Some("sectionProceduresHx")),
    group(Annex::Iv, PS, "immunisations", Some("sectionImmunizations")),
    group(
        Annex::Iv,
        PS,
        "functionalStatus",
        Some("sectionFunctionalStatus"),
    ),
    group(Annex::Iv, PS, "socialHistory", Some("sectionSocialHistory")),
    group(
        Annex::Iv,
        PS,
        "pregnancyHistory",
        Some("sectionPregnancyHx"),
    ),
    group(Annex::Iv, PS, "travelHistory", Some("sectionTravelHx")),
    group(Annex::Iv, PS, "patientStory", Some("sectionPatientStory")),
    group(
        Annex::Iv,
        PS,
        "advanceDirectives",
        Some("sectionAdvanceDirectives"),
    ),
    group(Annex::Iv, PS, "observationResults", Some("sectionResults")),
    group(Annex::Iv, PS, "carePlans", Some("sectionPlanOfCare")),
    group(Annex::Vii, IR, "body.orderInformation", Some("order")),
    group(
        Annex::Vii,
        IR,
        "body.supportingInformation",
        Some("history"),
    ),
    // NOTE: HL7 Europe Imaging Report 1.0.0-ballot CompositionEuImaging: the radiation dose is an
    // entry of the procedure section (`radiation-dose`), as the imaging crosswalk sets it.
    group(
        Annex::Vii,
        IR,
        "body.exposureInformation",
        Some("procedure"),
    ),
    group(Annex::Vii, IR, "body.examinationReport", Some("procedure")),
    group(
        Annex::Vii,
        IR,
        "body.recommendation",
        Some("recommendation"),
    ),
    // NOTE: HL7 Europe Laboratory Report 2.1.0 Composition-eu-lab slices only its annotations;
    // the order and its supporting information are DiagnosticReport.basedOn.
    group(Annex::Viii, LR, "body.orderInformation", None),
    group(Annex::Viii, LR, "body.supportingInformation", None),
    group(Annex::Viii, LR, "body.resultData", None),
    group(Annex::Ix, DR, "body.alerts", Some("sectionAlert")),
    // NOTE: HL7 Europe Hospital Discharge Report 1.0.0 composition-eu-hdr: the encounter is
    // Composition.encounter, which no section narrates.
    group(Annex::Ix, DR, "body.encounterInformation", None),
    group(
        Annex::Ix,
        DR,
        "body.admissionEvaluation",
        Some("sectionAdmissionEvaluation"),
    ),
    group(
        Annex::Ix,
        DR,
        "body.patientHistory",
        Some("sectionPatientHx"),
    ),
    group(
        Annex::Ix,
        DR,
        "body.courseOfEncounter",
        Some("sectionHospitalCourse"),
    ),
    group(
        Annex::Ix,
        DR,
        "body.dischargeDetails",
        Some("sectionDischargeDetails"),
    ),
    group(
        Annex::Ix,
        DR,
        "body.medicationSummary",
        Some("sectionDischargeMedications"),
    ),
    group(Annex::Ix, DR, "body.carePlan", Some("sectionPlanOfCare")),
];
