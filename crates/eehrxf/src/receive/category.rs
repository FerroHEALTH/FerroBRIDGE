// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The priority category of a received document, and the profiles it is
//! checked against.
//!
//! Each HL7 Europe document profile fixes the category of its documents in
//! the `Composition`: in `Composition.type`, by a `patternCodeableConcept`
//! such as LOINC `60591-5` in `composition-eu-eps` or by a binding to a
//! value set of one concept such as LOINC `11502-2` in `Composition-eu-lab`,
//! or, where the type is left open, by a required `Composition.category`
//! slice, such as `Medical-Imaging` in `CompositionEuImaging`. A document
//! says its category in the element its profile constrains
//! (<https://hl7.org/fhir/R4/profiling.html>).
//! [`ReceivedDocument::category`] reads those elements against the
//! categories this build receives, and [`Category::profiles`] names the
//! `Bundle` and `Composition` profiles [`ReceivedDocument::check`] holds it
//! to.

use fhir_types::r4::coding::Coding;

use crate::category::Category;
#[cfg(feature = "imaging")]
use crate::category::PRIORITY_SYSTEM;
use crate::receive::ReceivedDocument;

/// The LOINC code system (<https://hl7.org/fhir/R4/loinc.html>).
pub const LOINC: &str = "http://loinc.org";

/// The coding a category's document profile fixes, in `Composition.type` or
/// in a required `Composition.category` slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentType {
    /// The code system.
    pub system: &'static str,
    /// The code.
    pub code: &'static str,
}

impl DocumentType {
    /// Returns whether `coding` carries this system and this code.
    fn names(self, coding: &Coding) -> bool {
        coding
            .system
            .as_ref()
            .and_then(|system| system.value.as_deref())
            == Some(self.system)
            && coding.code.as_ref().and_then(|code| code.value.as_deref()) == Some(self.code)
    }
}

/// The `Bundle` and `Composition` profiles of a category's documents, by
/// canonical URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profiles {
    /// The `Bundle` profile.
    pub bundle: &'static str,
    /// The `Composition` profile.
    pub composition: &'static str,
}

impl Category {
    /// Returns the `Composition.type` the category's document profile fixes,
    /// when this build receives documents of the category and the profile
    /// fixes one.
    ///
    /// A prescription and a dispensation give `None` in every build: HL7
    /// Europe MPD 1.0.0 has no `Bundle` or `Composition` profile. An imaging
    /// report gives `None` too, because its profile leaves the type open;
    /// [`Category::document_category`] names it.
    #[must_use]
    pub const fn document_type(self) -> Option<DocumentType> {
        match self {
            // NOTE: HL7 Europe Patient Summary 1.0.0-ballot, composition-eu-eps: the
            // profile fixes Composition.type to LOINC 60591-5.
            #[cfg(feature = "patient-summary")]
            Self::PatientSummary => Some(DocumentType {
                system: LOINC,
                code: "60591-5",
            }),
            // NOTE: HL7 Europe Laboratory Report 2.0.0, Composition-eu-lab binds Composition.type to
            // lab-reportType-eu-lab, whose one concept is LOINC 11502-2.
            #[cfg(feature = "laboratory")]
            Self::Laboratory => Some(DocumentType {
                system: LOINC,
                code: "11502-2",
            }),
            #[cfg_attr(
                not(any(
                    feature = "prescription",
                    feature = "dispensation",
                    feature = "imaging",
                    feature = "discharge",
                )),
                expect(
                    unreachable_patterns,
                    reason = "a build with none of these features carries at most the patient summary \
                              and the laboratory report, which their own arms receive"
                )
            )]
            _ => None,
        }
    }

    /// Returns the `Composition.category` coding the category's document
    /// profile requires by a slice, when this build receives documents of
    /// the category and its profile leaves `Composition.type` open.
    #[must_use]
    pub const fn document_category(self) -> Option<DocumentType> {
        match self {
            // NOTE: HL7 Europe Imaging Report 1.0.0-ballot, CompositionEuImaging binds Composition.type
            // at strength preferred, and its required slice category:imaging fixes Medical-Imaging.
            #[cfg(feature = "imaging")]
            Self::Imaging => Some(DocumentType {
                system: PRIORITY_SYSTEM,
                code: "Medical-Imaging",
            }),
            #[cfg_attr(
                not(any(
                    feature = "patient-summary",
                    feature = "prescription",
                    feature = "dispensation",
                    feature = "laboratory",
                    feature = "discharge",
                )),
                expect(
                    unreachable_patterns,
                    reason = "a build with none of these features carries at most the imaging report, \
                              which its own arm receives"
                )
            )]
            _ => None,
        }
    }

    /// Returns the profiles a document of the category is checked against,
    /// when this build receives documents of the category.
    ///
    /// A prescription and a dispensation give `None` in every build: HL7
    /// Europe MPD 1.0.0 has no `Bundle` or `Composition` profile.
    #[must_use]
    pub const fn profiles(self) -> Option<Profiles> {
        match self {
            #[cfg(feature = "patient-summary")]
            Self::PatientSummary => Some(Profiles {
                bundle: "http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps",
                composition: "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps",
            }),
            #[cfg(feature = "imaging")]
            Self::Imaging => Some(Profiles {
                bundle: "http://hl7.eu/fhir/imaging/StructureDefinition/BundleReportEuImaging",
                composition: "http://hl7.eu/fhir/imaging/StructureDefinition/CompositionEuImaging",
            }),
            #[cfg(feature = "laboratory")]
            Self::Laboratory => Some(Profiles {
                bundle: "http://hl7.eu/fhir/laboratory/StructureDefinition/Bundle-eu-lab",
                composition: "http://hl7.eu/fhir/laboratory/StructureDefinition/Composition-eu-lab",
            }),
            #[cfg_attr(
                not(any(
                    feature = "prescription",
                    feature = "dispensation",
                    feature = "discharge",
                )),
                expect(
                    unreachable_patterns,
                    reason = "a build with none of these features carries at most the patient summary, \
                              the imaging report and the laboratory report, which their own arms receive"
                )
            )]
            _ => None,
        }
    }
}

impl ReceivedDocument {
    /// Returns the category of the document: the one category this build
    /// receives whose document type a `Composition.type` coding carries, or
    /// whose document category a `Composition.category` coding carries.
    ///
    /// # Errors
    ///
    /// Returns [`Uncategorised::Unknown`] when no coding names a category
    /// this build receives, and [`Uncategorised::Several`] when the codings
    /// name more than one.
    pub fn category(&self) -> Result<Category, Uncategorised> {
        let composition = self.composition();
        let types = &composition.r#type.coding;
        let categories: Vec<&Coding> = composition
            .category
            .iter()
            .flat_map(|category| &category.coding)
            .collect();
        let mut named = Category::ENABLED.iter().copied().filter(|category| {
            category
                .document_type()
                .is_some_and(|wanted| types.iter().any(|coding| wanted.names(coding)))
                || category
                    .document_category()
                    .is_some_and(|wanted| categories.iter().any(|coding| wanted.names(coding)))
        });
        match (named.next(), named.next()) {
            (Some(category), None) => Ok(category),
            (Some(_), Some(_)) => Err(Uncategorised::Several),
            (None, _) => Err(Uncategorised::Unknown),
        }
    }
}

/// Why a received document is of no one category this build receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Uncategorised {
    /// No `Composition.type` or `Composition.category` coding names a
    /// category this build receives.
    #[error("the document's Composition names no category this system receives")]
    Unknown,
    /// The `Composition.type` and `Composition.category` codings name more
    /// than one category.
    #[error("the document's Composition names more than one category")]
    Several,
}
