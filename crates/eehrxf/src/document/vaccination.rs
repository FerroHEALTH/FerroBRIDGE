// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The vaccination history in the MyHealth@EU Vaccination 10.0.0-preview
//! form (feature `vaccination`).
//!
//! MyHealth@EU Requirements Catalogue v10.0.0 §1.24 asks the contact point of
//! the country of affiliation to make a patient's vaccination history
//! available to a health professional. Vaccination is none of the priority
//! categories of Regulation (EU) 2025/327 Art 14(1), so the form is the
//! MyHealth@EU guide's own: a `bundle-euvac-myhealtheu` whose
//! `composition-euvac-myhealtheu` is of type LOINC 11369-6, holds the one
//! required section `immunisations`, and is legally authenticated
//! (`Composition.attester:legalAuthenticator` 1..1), and whose
//! `Bundle.entry:immunisation` slice requires at least one
//! `vaccination-myhealtheu-euvac`.

use crate::document::form::{Authorship, Code, Form, Kind, SectionRule, Slot};
use crate::document::language::{EHDSI_LANGUAGE, Language};
use crate::document::{LOINC, MYHEALTHEU_PATIENT};

/// The canonical URL of the MyHealth@EU Vaccination `Bundle` profile.
pub const EUVAC_BUNDLE: &str =
    "http://fhir.ehdsi.eu/euvac/StructureDefinition/bundle-euvac-myhealtheu";

/// The canonical URL of the MyHealth@EU Vaccination `Composition` profile.
pub const EUVAC_COMPOSITION: &str =
    "http://fhir.ehdsi.eu/euvac/StructureDefinition/composition-euvac-myhealtheu";

/// The LOINC code `composition-euvac-myhealtheu` fixes as
/// `Composition.type` and as the code of its immunisations section.
pub const IMMUNIZATION_HISTORY: &str = "11369-6";

/// The one section slice of `composition-euvac-myhealtheu`.
pub const EUVAC_SLOTS: [Slot; 1] = [Slot::new(
    "immunisations",
    IMMUNIZATION_HISTORY,
    "Immunisations",
)];

/// The vaccination history in the MyHealth@EU Vaccination 10.0.0-preview
/// form.
pub const VACCINATION_HISTORY_FORM: Form = Form {
    bundle: EUVAC_BUNDLE,
    composition: EUVAC_COMPOSITION,
    patient: MYHEALTHEU_PATIENT,
    kind: Kind::Fixed(Code {
        system: LOINC,
        code: IMMUNIZATION_HISTORY,
    }),
    categories: &[],
    authorship: Authorship::Gateway,
    slots: &EUVAC_SLOTS,
    report: None,
    encounter: None,
    // NOTE: composition-euvac-myhealtheu Composition.language 0..1, bound required to eHDSILanguage.
    language: Language {
        required: false,
        binding: Some(EHDSI_LANGUAGE),
    },
    // NOTE: composition-euvac-myhealtheu section:immunisations 1..1, and bundle-euvac-myhealtheu
    // entry:immunisation 1..* holds the Immunization resources that section lists.
    rules: &[
        SectionRule::Required {
            slices: &["immunisations"],
        },
        SectionRule::Entered {
            entry: "immunisation",
            slice: "immunisations",
        },
    ],
    entries: &[],
    legal_authentication: true,
};
