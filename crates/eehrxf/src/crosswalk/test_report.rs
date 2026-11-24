// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The crosswalk of the non-laboratory medical test report (feature
//! `laboratory`).
//!
//! Annex VIII of the draft `EEHRxF` implementing act (initiative 17512,
//! 2026-11-23) defines one `EHDSMedicalTestReport` for laboratory and for
//! other diagnostic tests. No HL7 Europe guide profiles a non-laboratory
//! report ([`NON_LABORATORY_PROFILE`] is `None`), so each Annex VIII element
//! is set beside the R4 base element of the report's `Composition` or
//! `DiagnosticReport` that carries it, or none where R4 has no place for it.
//! The specimen is laboratory alone.

/// The HL7 Europe profile of a non-laboratory medical test report: none.
pub const NON_LABORATORY_PROFILE: Option<&str> = None;

/// One Annex VIII element of a non-laboratory medical test report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TestReportRow {
    /// The Annex VIII element, as the shipped draft crosswalk spells it.
    pub element: &'static str,
    /// Whether the element applies to a non-laboratory report.
    pub applies: bool,
    /// The R4 base element that carries it, or `None` where R4 has none.
    pub fhir: Option<&'static str>,
}

/// Creates the row of `element`, which applies, carried by `fhir`.
const fn carried(element: &'static str, fhir: &'static str) -> TestReportRow {
    TestReportRow {
        element,
        applies: true,
        fhir: Some(fhir),
    }
}

/// Creates the row of `element`, which applies and has no R4 element.
const fn unplaced(element: &'static str) -> TestReportRow {
    TestReportRow {
        element,
        applies: true,
        fhir: None,
    }
}

/// Every element of Annex VIII, in the annex's order.
pub const TEST_REPORT_ROWS: &[TestReportRow] = &[
    carried("header", "Composition"),
    carried("header.subject", "Composition.subject"),
    carried("header.identifier", "Composition.identifier"),
    carried("header.version", "Composition.extension"),
    carried("header.author[x]", "Composition.author"),
    carried("header.date", "Composition.date"),
    carried("header.status", "Composition.status"),
    carried("header.language", "Composition.language"),
    carried("header.documentCategory", "Composition.category"),
    carried("header.documentType", "Composition.type"),
    carried("header.documentTitle", "Composition.title"),
    carried("header.period", "Composition.event.period"),
    carried("header.attestation", "Composition.attester"),
    carried(
        "header.attestation.attester[x]",
        "Composition.attester.party",
    ),
    carried("header.attestation.dateTime", "Composition.attester.time"),
    carried("header.legalAuthentication", "Composition.attester"),
    carried(
        "header.legalAuthentication.legalAuthenticator[x]",
        "Composition.attester.party",
    ),
    carried(
        "header.legalAuthentication.dateTime",
        "Composition.attester.time",
    ),
    carried("header.eventCategory", "Composition.event.code"),
    unplaced("header.serviceSpecialty"),
    carried("header.custodian", "Composition.custodian"),
    unplaced("header.countryOfOrigin"),
    carried("presentedForm", "DiagnosticReport.presentedForm"),
    unplaced("intendedRecipient[x]"),
    carried("body", "DiagnosticReport"),
    carried("body.orderInformation", "DiagnosticReport.basedOn"),
    carried("body.orderInformation.orderId", "DiagnosticReport.basedOn"),
    carried(
        "body.orderInformation.orderDateAndTime",
        "DiagnosticReport.basedOn",
    ),
    carried(
        "body.orderInformation.orderPlacer[x]",
        "DiagnosticReport.basedOn",
    ),
    carried(
        "body.orderInformation.orderReason[x]",
        "DiagnosticReport.basedOn",
    ),
    carried(
        "body.orderInformation.clinicalQuestion",
        "DiagnosticReport.basedOn",
    ),
    unplaced("body.supportingInformation"),
    unplaced("body.supportingInformation.observation"),
    unplaced("body.supportingInformation.condition"),
    unplaced("body.supportingInformation.medicationAdministration"),
    unplaced("body.supportingInformation.immunisation"),
    TestReportRow {
        element: "body.specimen",
        applies: false,
        fhir: None,
    },
    carried("body.resultData", "DiagnosticReport"),
    carried("body.resultData.comment", "DiagnosticReport.conclusion"),
    carried(
        "body.resultData.medicalTestResult[x]",
        "DiagnosticReport.result",
    ),
    carried("attachment", "DiagnosticReport.media"),
];
