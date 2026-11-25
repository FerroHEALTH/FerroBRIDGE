// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The technical representations of draft `EEHRxF` Art 3(1) and Annex II.
//!
//! Art 3(1) of the draft implementing act (initiative 17512, 2026-11-23)
//! exchanges a document as (a) a PDF attachment embedded in a FHIR resource
//! with its metadata, (b) FHIR narrative organised by data group, (c)
//! structured FHIR with that narrative, or (d) for imaging, DICOM with a
//! FHIR report and manifest; Annex II admits PDF/A-1a or PDF/A-1b alone and
//! lists which representations each category may take. [`Declarations`]
//! is the configuration of what a system declares per category, held to
//! Annex II; [`PdfA`] is a PDF the component accepts for (a), which
//! [`DocumentMetadata::describe_pdf`] embeds; [`Document::narrative_only`]
//! writes (b).
//!
//! [`DocumentMetadata::describe_pdf`]: crate::document::metadata::DocumentMetadata::describe_pdf

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::category::Category;
use crate::document::{Document, DocumentError};

/// One technical representation of draft `EEHRxF` Art 3(1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Representation {
    /// Art 3(1)(a): a PDF/A-1 document embedded in a FHIR resource.
    Pdf,
    /// Art 3(1)(b): FHIR narrative organised by data group.
    Narrative,
    /// Art 3(1)(c): structured FHIR with its narrative.
    Structured,
    /// Art 3(1)(d): DICOM with a FHIR report and manifest.
    Dicom,
}

/// The representations Annex II admits for `category`.
#[must_use]
pub fn admitted(
    #[cfg_attr(
        not(feature = "imaging"),
        expect(unused_variables, reason = "only imaging admits another set")
    )]
    category: Category,
) -> &'static [Representation] {
    #[cfg(feature = "imaging")]
    if category == Category::Imaging {
        return &[
            Representation::Pdf,
            Representation::Narrative,
            Representation::Structured,
            Representation::Dicom,
        ];
    }
    &[
        Representation::Pdf,
        Representation::Narrative,
        Representation::Structured,
    ]
}

/// What a system declares per category, the configuration Art 3(1) leaves
/// to it, keyed by the category's code in HL7 Europe's
/// `EEHRxFDocumentPriorityCategoryCS` ([`Category::code`]).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(transparent)]
pub struct Declarations(BTreeMap<String, BTreeSet<Representation>>);

impl Declarations {
    /// Returns the declarations of a system that takes structured FHIR, and
    /// for imaging DICOM too, for every category this build carries.
    #[must_use]
    pub fn structured() -> Self {
        let mut declared = BTreeMap::new();
        for category in Category::ENABLED {
            let mut set = BTreeSet::from([Representation::Structured]);
            if admitted(*category).contains(&Representation::Dicom) {
                set.insert(Representation::Dicom);
            }
            declared.insert(category.code().to_owned(), set);
        }
        Self(declared)
    }

    /// Returns whether `representation` is declared for `category`.
    #[must_use]
    pub fn declares(&self, category: Category, representation: Representation) -> bool {
        self.0
            .get(category.code())
            .is_some_and(|set| set.contains(&representation))
    }

    /// Holds the declarations to Annex II and to this build.
    ///
    /// # Errors
    ///
    /// [`RepresentationError::Category`] for a key that names no category
    /// this build carries, and [`RepresentationError::NotAdmitted`] for a
    /// representation Annex II does not admit for its category.
    pub fn check(&self) -> Result<(), RepresentationError> {
        for (key, set) in &self.0 {
            let category =
                Category::of_code(key).ok_or_else(|| RepresentationError::Category(key.clone()))?;
            if let Some(refused) = set.iter().find(|r| !admitted(category).contains(r)) {
                return Err(RepresentationError::NotAdmitted {
                    category: key.clone(),
                    representation: *refused,
                });
            }
        }
        Ok(())
    }
}

/// Why a representation cannot be written or declared.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RepresentationError {
    /// A declaration names no category this build carries.
    #[error("{0:?} is no priority category of this build")]
    Category(String),
    /// Annex II admits no such representation for the category.
    #[error("Annex II does not admit {representation:?} for {category}")]
    NotAdmitted {
        /// The category key.
        category: String,
        /// The refused representation.
        representation: Representation,
    },
    /// The bytes are no PDF.
    #[error("the bytes are no PDF document")]
    NotPdf,
    /// The PDF declares no PDF/A-1a or PDF/A-1b conformance.
    #[error("the PDF declares no PDF/A-1a or PDF/A-1b conformance (found {found})")]
    NotPdfA1 {
        /// The part and conformance its metadata declares, or `none`.
        found: String,
    },
}

/// The conformance level of a PDF/A-1 document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conformance {
    /// PDF/A-1a, accessible.
    A,
    /// PDF/A-1b, basic.
    B,
}

/// A PDF/A-1a or PDF/A-1b document, its bytes as given.
#[derive(Clone, PartialEq, Eq)]
pub struct PdfA {
    bytes: Vec<u8>,
    conformance: Conformance,
}

impl std::fmt::Debug for PdfA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfA")
            .field("size", &self.bytes.len())
            .field("conformance", &self.conformance)
            .finish()
    }
}

impl PdfA {
    /// Accepts `bytes` as a PDF/A-1 document by the identification schema
    /// its XMP metadata carries (`pdfaid:part` 1, `pdfaid:conformance` A or
    /// B), the bytes kept unchanged.
    ///
    /// # Errors
    ///
    /// [`RepresentationError::NotPdf`] for bytes that do not open with the
    /// PDF header, and [`RepresentationError::NotPdfA1`] for a PDF whose
    /// metadata declares another part or level, or none.
    pub fn new(bytes: Vec<u8>) -> Result<Self, RepresentationError> {
        if !bytes.starts_with(b"%PDF-") {
            return Err(RepresentationError::NotPdf);
        }
        // NOTE: ISO 19005-1 6.7.11: the PDF/A identification schema, whose metadata stream
        // PDF/A-1 keeps unfiltered (6.7.2), so its properties read as plain text.
        let text = String::from_utf8_lossy(&bytes);
        let part = property(&text, "pdfaid:part");
        let level = property(&text, "pdfaid:conformance");
        let conformance = match (part.as_deref(), level.as_deref()) {
            (Some("1"), Some("A")) => Conformance::A,
            (Some("1"), Some("B")) => Conformance::B,
            (part, level) => {
                return Err(RepresentationError::NotPdfA1 {
                    found: format!(
                        "part {}, level {}",
                        part.unwrap_or("none"),
                        level.unwrap_or("none")
                    ),
                });
            }
        };
        Ok(Self { bytes, conformance })
    }

    /// Returns the bytes as given.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the conformance level.
    #[must_use]
    pub const fn conformance(&self) -> Conformance {
        self.conformance
    }
}

/// The value of the XMP property `name` in `text`, written as an attribute
/// (`name="1"`) or as an element (`<name>1</name>`).
fn property(text: &str, name: &str) -> Option<String> {
    let attribute = format!("{name}=");
    if let Some(at) = text.find(&attribute) {
        let rest = text.get(at + attribute.len()..)?;
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let value = rest.get(1..)?;
        return value.split(quote).next().map(|v| v.trim().to_owned());
    }
    let open = format!("<{name}>");
    let at = text.find(&open)?;
    let rest = text.get(at + open.len()..)?;
    rest.split('<').next().map(|v| v.trim().to_owned())
}

impl Document {
    /// Writes the document in the representation of draft `EEHRxF` Art
    /// 3(1)(b): narrative by data group and no structured entries, so a
    /// section given an entry is refused when the document is written.
    pub fn narrative_only(&mut self) {
        self.narrative_only = true;
    }

    /// Refuses a section with an entry in a narrative-only document.
    pub(super) fn hold_representation(&self) -> Result<(), DocumentError> {
        match self
            .sections
            .iter()
            .find(|section| self.narrative_only && !section.entries.is_empty())
        {
            Some(section) => Err(DocumentError::NarrativeOnly {
                slice: section.slot.slice,
            }),
            None => Ok(()),
        }
    }
}
