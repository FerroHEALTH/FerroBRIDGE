// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The display of each code a document writes: its type, its sections,
//! their empty reasons and its priority category.
//!
//! FHIR R4 `Coding.display` is "a representation of the meaning of the code
//! in the system, following the rules of the system"
//! (<https://hl7.org/fhir/R4/datatypes.html#Coding>), and a receiver with no
//! terminology server reads the meaning from it alone. The LOINC tables are
//! not vendored, so each display is the one the vendored European material
//! gives, in this order: a value set of the MyHealth@EU Master Value Sets
//! Catalogue 9.1.0; a value set or code system of an HL7 Europe guide the
//! crate pins, the EU Health Data API among them; an English example of one
//! of those guides. A code none of them names carries no display rather
//! than one written from memory.

use super::{EMPTY_REASON, LOINC};
use crate::category::PRIORITY_SYSTEM;

/// The LOINC displays, by code, in code order.
const LOINC_DISPLAYS: &[(&str, &str)] = &[
    ("10160-0", "History of Medication use Narrative"),
    ("10162-6", "History of pregnancies Narrative"),
    ("10182-4", "History of Travel note"),
    ("10185-7", "Hospital discharge procedures Narrative"),
    ("104605-1", "Alert"),
    ("11329-0", "History"),
    ("11369-6", "History of Immunization Narrative"),
    ("11450-4", "Problem list - Reported"),
    ("11502-2", "Laboratory report"),
    ("11535-2", "Hospital discharge Dx Narrative"),
    ("18717-9", "Blood bank studies (set)"),
    ("18719-5", "Chemistry studies (set)"),
    ("18722-9", "Fertility studies (set)"),
    ("18723-7", "Hematology studies (set)"),
    ("18725-2", "Microbiology studies (set)"),
    ("18726-0", "Imaging Study"),
    ("18728-6", "Toxicology studies (set)"),
    ("18776-5", "Plan of care note"),
    ("18783-1", "Recommendation"),
    ("18834-2", "Comparison"),
    ("18842-5", "Discharge summary"),
    (
        "19005-8",
        "Radiology Imaging study [Impression] (narrative)",
    ),
    ("26436-6", "Laboratory studies (set)"),
    ("29545-1", "Physical findings Narrative"),
    ("29762-2", "Social history Narrative"),
    (
        "30954-2",
        "Relevant diagnostic tests/laboratory data Narrative",
    ),
    ("34105-7", "Hospital Discharge summary"),
    ("42348-3", "Advance healthcare directives"),
    ("46264-8", "History of medical device use"),
    ("47420-5", "Functional status assessment note"),
    ("47519-4", "History of Procedures Document"),
    ("48765-2", "Allergies and adverse reactions Document"),
    ("55111-9", "Procedure"),
    ("55115-0", "Order"),
    ("59776-5", "Findings"),
    ("60591-5", "Patient summary Document"),
    ("67781-5", "Summarization of encounter note Narrative"),
    ("67851-6", "Admission evaluation note"),
    ("73568-8", "Communication"),
    ("75311-1", "Discharge medications Narrative"),
    ("85430-7", "Diagnostic imaging report"),
    ("8648-8", "Hospital course Narrative"),
    ("8650-4", "Hospital discharge disposition Narrative"),
    ("8716-3", "Vital signs"),
    ("87232-5", "Medication administration.brief"),
];

/// The displays of the FHIR R4 `list-empty-reason` code system
/// (<https://hl7.org/fhir/R4/codesystem-list-empty-reason.html>), by code.
const EMPTY_REASON_DISPLAYS: &[(&str, &str)] = &[
    ("nilknown", "Nil Known"),
    ("notasked", "Not Asked"),
    ("withheld", "Information Withheld"),
    ("unavailable", "Unavailable"),
    ("notstarted", "Not Started"),
    ("closed", "Closed"),
];

/// The displays of HL7 Europe's `EEHRxFDocumentPriorityCategoryCS`
/// (`hl7.fhir.eu.health-data-api` 1.0.0-ballot), by code.
const PRIORITY_DISPLAYS: &[(&str, &str)] = &[
    ("Patient-Summaries", "patient summaries"),
    ("Electronic-Prescriptions", "electronic prescriptions"),
    ("Electronic-Dispensations", "electronic dispensations"),
    (
        "Medical-Imaging",
        "medical imaging studies and related imaging reports",
    ),
    (
        "Laboratory-Reports",
        "medical test results, including laboratory and other diagnostic results and related reports",
    ),
    ("Discharge-Reports", "discharge reports"),
];

/// Returns the display of `code` of the code system `system`, or `None` when
/// no vendored source names one.
#[must_use]
pub fn display(system: &str, code: &str) -> Option<&'static str> {
    let displays = match system {
        LOINC => LOINC_DISPLAYS,
        EMPTY_REASON => EMPTY_REASON_DISPLAYS,
        PRIORITY_SYSTEM => PRIORITY_DISPLAYS,
        _ => return None,
    };
    displays
        .iter()
        .find(|(held, _)| *held == code)
        .map(|(_, display)| *display)
}
