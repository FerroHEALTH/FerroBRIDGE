// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The discharge report in the HL7 Europe Hospital Discharge Report 1.0.0
//! form and in the MyHealth@EU Hospital Discharge Report 9.2.0 form (feature
//! `discharge`).
//!
//! A `bundle-eu-hdr` whose `composition-eu-hdr` names the encounter it
//! reports on, in the language `Bundle.language` states. The MyHealth@EU
//! form, which a national contact point sends, narrows it: its
//! `composition-hdr-myhealtheu` requires `Composition.language` from the
//! eHDSILanguage value set and allows only the authors the record names, and
//! its `medicationRequest-hdr-myhealtheu` requires each order's authoring
//! time and requester.

use crate::crosswalk::discharge::HDR_COMPOSITION;
use crate::document::form::{Authorship, Code, EntryRule, Form, Kind, SectionRule, Slot};
use crate::document::language::{EHDSI_LANGUAGE, Language};
use crate::document::{EU_CORE_PATIENT, LOINC, MYHEALTHEU_PATIENT};

/// The canonical URL of the HL7 Europe Hospital Discharge Report `Bundle`
/// profile.
pub const HDR_BUNDLE: &str = "http://hl7.eu/fhir/hdr/StructureDefinition/bundle-eu-hdr";

/// The canonical URL of the HL7 Europe Hospital Discharge Report
/// `Encounter` profile, the one `composition-eu-hdr` takes as its encounter.
pub const HDR_ENCOUNTER: &str = "http://hl7.eu/fhir/hdr/StructureDefinition/encounter-eu-hdr";

/// The canonical URL of the MyHealth@EU Hospital Discharge Report `Bundle`
/// profile.
pub const MYHEALTHEU_HDR_BUNDLE: &str =
    "http://fhir.ehdsi.eu/hdr/StructureDefinition/bundle-hdr-myhealtheu";

/// The canonical URL of the MyHealth@EU Hospital Discharge Report
/// `Composition` profile.
pub const MYHEALTHEU_HDR_COMPOSITION: &str =
    "http://fhir.ehdsi.eu/hdr/StructureDefinition/composition-hdr-myhealtheu";

/// The canonical URL of the MyHealth@EU Hospital Discharge Report
/// `Encounter` profile, the one `composition-hdr-myhealtheu` takes as its
/// encounter.
pub const MYHEALTHEU_HDR_ENCOUNTER: &str =
    "http://fhir.ehdsi.eu/hdr/StructureDefinition/encounter-hdr-myhealtheu";

/// The canonical URL of the MyHealth@EU Hospital Discharge Report
/// `MedicationRequest` profile, which `composition-hdr-myhealtheu` names for
/// the discharge medications section's entries.
pub const MYHEALTHEU_HDR_MEDICATION_REQUEST: &str =
    "http://fhir.ehdsi.eu/hdr/StructureDefinition/medicationRequest-hdr-myhealtheu";

/// The LOINC code `composition-eu-hdr` fixes as `Composition.type`, a
/// discharge summary.
pub const DISCHARGE_SUMMARY: &str = "34105-7";

/// Every section slice of `composition-eu-hdr`, in the profile's order.
pub const HDR_SLOTS: [Slot; 23] = [
    Slot::new("sectionAlert", "104605-1", "Alerts"),
    Slot::new(
        "sectionAdmissionEvaluation",
        "67851-6",
        "Admission evaluation",
    ),
    Slot::new("sectionVitalSigns", "8716-3", "Vital signs"),
    Slot::new("sectionPhysicalFindings", "29545-1", "Physical findings"),
    Slot::new("sectionFunctionalStatus", "47420-5", "Functional status"),
    Slot::new("sectionDischargeSummary", "18842-5", "Discharge summary"),
    Slot::new("sectionHospitalCourse", "8648-8", "Hospital course"),
    Slot::new("sectionDiagnosticSummary", "11535-2", "Diagnostic summary"),
    Slot::new(
        "sectionSignificantProcedures",
        "10185-7",
        "Significant procedures",
    ),
    Slot::new("sectionMedicalDevices", "46264-8", "Medical devices"),
    Slot::new("sectionProceduresHx", "47519-4", "History of procedures"),
    Slot::new("sectionPharmacotherapy", "87232-5", "Pharmacotherapy"),
    Slot::new("sectionImplantedDevices", "57080-4", "Implanted devices"),
    Slot::new(
        "sectionSignificantResults",
        "30954-2",
        "Significant results",
    ),
    Slot::new("sectionSynthesis", "67781-5", "Synthesis"),
    Slot::new("sectionPlanOfCare", "18776-5", "Plan of care"),
    Slot::new(
        "sectionDischargeMedications",
        "75311-1",
        "Discharge medications",
    ),
    Slot::new("sectionAllergies", "48765-2", "Allergies and intolerances"),
    Slot::new("sectionPatientHx", "11329-0", "Patient history"),
    Slot::new("sectionProblems", "11450-4", "Problems"),
    Slot::new("sectionImmunizations", "11369-6", "Immunizations"),
    Slot::new("sectionDischargeDetails", "8650-4", "Discharge details"),
    Slot::new("sectionAttachments", "77599-9", "Attachments"),
];

/// The discharge report in the HL7 Europe Hospital Discharge Report 1.0.0
/// form.
pub const DISCHARGE_REPORT_FORM: Form = Form {
    bundle: HDR_BUNDLE,
    composition: HDR_COMPOSITION,
    patient: EU_CORE_PATIENT,
    kind: Kind::Fixed(Code {
        system: LOINC,
        code: DISCHARGE_SUMMARY,
    }),
    categories: &[],
    authorship: Authorship::Gateway,
    slots: &HDR_SLOTS,
    report: None,
    encounter: Some(HDR_ENCOUNTER),
    language: Language::REQUIRED,
    // NOTE: composition-eu-hdr `cmp-hdr-2` (error): a discharge summary or a hospital course section.
    rules: &[SectionRule::OneOf {
        invariant: "cmp-hdr-2",
        slices: &["sectionDischargeSummary", "sectionHospitalCourse"],
    }],
    entries: &[],
    legal_authentication: false,
};

/// The discharge report in the MyHealth@EU Hospital Discharge Report 9.2.0
/// form, the HL7 Europe form narrowed by `composition-hdr-myhealtheu`.
pub const MYHEALTHEU_DISCHARGE_REPORT_FORM: Form = Form {
    bundle: MYHEALTHEU_HDR_BUNDLE,
    composition: MYHEALTHEU_HDR_COMPOSITION,
    patient: MYHEALTHEU_PATIENT,
    encounter: Some(MYHEALTHEU_HDR_ENCOUNTER),
    // NOTE: composition-hdr-myhealtheu Composition.language 1..1, bound required to eHDSILanguage.
    language: Language {
        required: true,
        binding: Some(EHDSI_LANGUAGE),
    },
    // NOTE: composition-hdr-myhealtheu Composition.author: practitioner, practitionerRole or organization.
    authorship: Authorship::Record,
    // NOTE: medicationRequest-hdr-myhealtheu MedicationRequest.authoredOn and requester, each 1..1.
    entries: &[EntryRule {
        resource: "MedicationRequest",
        profile: MYHEALTHEU_HDR_MEDICATION_REQUEST,
        elements: &["authoredOn", "requester"],
    }],
    ..DISCHARGE_REPORT_FORM
};
