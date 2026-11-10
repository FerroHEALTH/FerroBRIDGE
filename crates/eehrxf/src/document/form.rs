// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The forms a document takes: its section slots, the codes it carries, and
//! the companions and authorship each form names.

#[cfg(feature = "patient-summary")]
use super::{EPS_BUNDLE, EPS_PATIENT, LOINC, PATIENT_SUMMARY};

/// One section slice of the `composition-eu-eps` profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot {
    /// The slice name, such as `sectionAllergies`.
    pub slice: &'static str,
    /// The LOINC code the slice's `code` pattern fixes.
    pub code: &'static str,
    /// The section title the document writes.
    pub title: &'static str,
}

impl Slot {
    /// Creates the slot of the slice `slice`, coded `code`, titled `title`.
    #[must_use]
    pub const fn new(slice: &'static str, code: &'static str, title: &'static str) -> Self {
        Self { slice, code, title }
    }
}

/// Every section slice of the `composition-eu-eps` profile, in the profile's
/// order.
#[cfg(feature = "patient-summary")]
pub const SLOTS: [Slot; 17] = [
    Slot::new("sectionProblems", "11450-4", "Problems"),
    Slot::new("sectionAllergies", "48765-2", "Allergies and Intolerances"),
    Slot::new("sectionMedications", "10160-0", "Medication Summary"),
    Slot::new("sectionImmunizations", "11369-6", "Immunizations"),
    Slot::new("sectionResults", "30954-2", "Results"),
    Slot::new("sectionProceduresHx", "47519-4", "History of Procedures"),
    Slot::new("sectionMedicalDevices", "46264-8", "Medical Devices"),
    Slot::new("sectionAdvanceDirectives", "42348-3", "Advance Directives"),
    Slot::new("sectionAlert", "104605-1", "Alerts"),
    Slot::new("sectionFunctionalStatus", "47420-5", "Functional Status"),
    Slot::new("sectionPregnancyHx", "10162-6", "History of Pregnancy"),
    Slot::new("sectionPatientStory", "81338-6", "Patient Story"),
    Slot::new("sectionPlanOfCare", "18776-5", "Plan of Care"),
    Slot::new("sectionSocialHistory", "29762-2", "Social History"),
    Slot::new("sectionVitalSigns", "8716-3", "Vital Signs"),
    Slot::new("sectionTravelHx", "10182-4", "Travel History"),
    Slot::new("sectionPatientHx", "11329-0", "Patient History"),
];

/// Returns the slot of the patient summary's section slice `slice`, or
/// `None` for a name the profile does not slice.
#[cfg(feature = "patient-summary")]
#[must_use]
pub fn slot(slice: &str) -> Option<&'static Slot> {
    SLOTS.iter().find(|slot| slot.slice == slice)
}

/// A code of a code system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Code {
    /// The code system.
    pub system: &'static str,
    /// The code.
    pub code: &'static str,
}

/// The report a form's `Bundle` holds beside its composition, each naming
/// the other by an extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Companion {
    /// The canonical URL of the report's `DiagnosticReport` profile, which
    /// the report claims.
    pub profile: &'static str,
    /// The URL of the `Composition` extension that names the report.
    pub link: &'static str,
    /// The URL of the `DiagnosticReport` extension that names the
    /// composition.
    pub back: &'static str,
}

/// Where a document's `Composition.type` comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The code the form fixes, written on the report too when the form has
    /// one.
    Fixed(Code),
    /// The codings of the report's `code`, or `fallback` when the report
    /// carries none, which the report then carries too.
    Report {
        /// The code written when the report carries none.
        fallback: Code,
    },
}

/// Who a document's composition names as its authors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorship {
    /// Every author the document was given ([`super::Document::with_author`]): the
    /// gateway `Device` and the operator `Organization`.
    Gateway,
    /// The given authors that are no `Organization`, and the `Organization`s
    /// the report names as its performers, as `BundleReportEuImaging`'s
    /// `dr-comp-author-org` requires; the first given `Organization`, the
    /// operator, is the composition's `custodian` unless the document names
    /// one ([`super::Document::with_custodian`]).
    ReportPerformers,
}

/// A rule of a form's `Composition` profile over the sections a document
/// holds, which [`super::Document::into_bundle`] holds the document to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SectionRule {
    /// The profile invariant `invariant`: a section of one of `slices` holds
    /// an entry or an empty reason.
    Reasoned {
        /// The key of the profile invariant.
        invariant: &'static str,
        /// The section slices the invariant constrains.
        slices: &'static [&'static str],
    },
    /// The profile's cardinality: the document holds a section of each of
    /// `slices`, the section slices whose `min` is 1.
    Required {
        /// The section slices the profile requires.
        slices: &'static [&'static str],
    },
    /// The profile invariant `invariant`: the document holds a section of at
    /// least one of `slices`.
    OneOf {
        /// The key of the profile invariant.
        invariant: &'static str,
        /// The section slices of which one is required.
        slices: &'static [&'static str],
    },
}

/// The form of one category's document: its HL7 Europe profiles, its
/// document type and its sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Form {
    /// The canonical URL of the `Bundle` profile the document claims.
    pub bundle: &'static str,
    /// The canonical URL of the `Composition` profile the bundle's
    /// composition slice names.
    pub composition: &'static str,
    /// The canonical URL of the `Patient` profile the subject claims.
    pub patient: &'static str,
    /// Where the `Composition.type` the document writes comes from.
    pub kind: Kind,
    /// The categories the composition, and the report when the form has
    /// one, carry.
    pub categories: &'static [Code],
    /// Who the composition names as its authors.
    pub authorship: Authorship,
    /// The sections a document of the form may hold, in the order the
    /// form lists them.
    pub slots: &'static [Slot],
    /// The report the `Bundle` holds beside the composition, when the form
    /// has one.
    pub report: Option<Companion>,
    /// The canonical URL of the `Encounter` profile, when the form requires
    /// the encounter the document is about.
    pub encounter: Option<&'static str>,
    /// Whether the form requires `Bundle.language`.
    pub language: bool,
    /// The rules of the `Composition` profile over the sections.
    pub rules: &'static [SectionRule],
}

/// The patient summary in the HL7 Europe Patient Summary 1.0.0-ballot form.
#[cfg(feature = "patient-summary")]
pub const PATIENT_SUMMARY_FORM: Form = Form {
    bundle: EPS_BUNDLE,
    composition: "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps",
    patient: EPS_PATIENT,
    kind: Kind::Fixed(Code {
        system: LOINC,
        code: PATIENT_SUMMARY,
    }),
    categories: &[],
    authorship: Authorship::Gateway,
    slots: &SLOTS,
    report: None,
    encounter: None,
    language: false,
    // NOTE: composition-eu-eps `ips-comp-1` (error) on these three slices: an entry or an emptyReason.
    rules: &[
        SectionRule::Reasoned {
            invariant: "ips-comp-1",
            slices: &["sectionProblems", "sectionAllergies", "sectionMedications"],
        },
        SectionRule::Required {
            slices: &[
                "sectionProblems",
                "sectionAllergies",
                "sectionMedications",
                "sectionProceduresHx",
                "sectionMedicalDevices",
            ],
        },
    ],
};
