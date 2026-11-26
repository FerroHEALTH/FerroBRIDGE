// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The non-laboratory medical test report of draft `EEHRxF` Annex VIII
//! (feature `laboratory`).
//!
//! Annex VIII of the draft `EEHRxF` implementing act (initiative 17512,
//! 2026-11-23) defines `EHDSMedicalTestReport` for laboratory and for other
//! diagnostic tests, such as spirometry (LOINC `18759-1`). No HL7 Europe
//! guide profiles a non-laboratory report, so the document is an R4
//! `document` `Bundle` whose `Composition` and `DiagnosticReport` each name
//! the other and claim the R4 base definitions alone; the composition's type
//! is the report's code.

use crate::document::LOINC;
use crate::document::form::{Authorship, Code, Companion, Form, Kind, Slot};
use crate::document::language::Language;

/// The R4 `Bundle` definition the document claims.
pub const TEST_REPORT_BUNDLE: &str = "http://hl7.org/fhir/StructureDefinition/Bundle";

/// The R4 `Composition` definition the document's composition claims.
pub const TEST_REPORT_COMPOSITION: &str = "http://hl7.org/fhir/StructureDefinition/Composition";

/// The R4 `DiagnosticReport` definition the report claims.
pub const TEST_REPORT_DIAGNOSTIC_REPORT: &str =
    "http://hl7.org/fhir/StructureDefinition/DiagnosticReport";

/// The R4 `Patient` definition the subject claims.
pub const TEST_REPORT_PATIENT: &str = "http://hl7.org/fhir/StructureDefinition/Patient";

/// The LOINC code of a spirometry report, the example Annex VIII names.
pub const SPIROMETRY: &str = "18759-1";

/// The LOINC code of an electrocardiogram report.
pub const ELECTROCARDIOGRAM: &str = "11524-6";

/// The `Composition.type` of a report whose `DiagnosticReport.code` carries
/// none: LOINC `47045-0`, a study report.
pub const STUDY_REPORT: &str = "47045-0";

/// The one section of a medical test report, the results the report holds.
pub const TEST_REPORT_SLOTS: [Slot; 1] = [Slot::new(
    "results",
    "30954-2",
    "Relevant diagnostic tests/laboratory data",
)];

/// The non-laboratory medical test report.
pub const TEST_REPORT_FORM: Form = Form {
    bundle: TEST_REPORT_BUNDLE,
    composition: TEST_REPORT_COMPOSITION,
    patient: TEST_REPORT_PATIENT,
    kind: Kind::Report {
        fallback: Code {
            system: LOINC,
            code: STUDY_REPORT,
        },
    },
    categories: &[],
    authorship: Authorship::Gateway,
    slots: &TEST_REPORT_SLOTS,
    report: Some(Companion {
        profile: TEST_REPORT_DIAGNOSTIC_REPORT,
        link: "http://hl7.eu/fhir/extensions/StructureDefinition/composition-diagnosticReportReference",
        back: "http://hl7.org/fhir/5.0/StructureDefinition/extension-DiagnosticReport.composition",
    }),
    encounter: None,
    language: Language::OPTIONAL,
    // NOTE: FHIR R4 Composition cmp-1 holds by the narrative every section carries.
    rules: &[],
    legal_authentication: false,
};
