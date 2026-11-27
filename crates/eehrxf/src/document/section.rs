// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One section of a document: its slot, its narrative, its entries and the
//! reason it has none.

use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::extension::Extension;

use super::form::Slot;

/// Why a section has no entry (FHIR R4 `list-empty-reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum EmptyReason {
    /// `nilknown`: there is nothing to record.
    NilKnown,
    /// `unavailable`: the information is not available.
    Unavailable,
}

impl EmptyReason {
    /// Returns the code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NilKnown => "nilknown",
            Self::Unavailable => "unavailable",
        }
    }
}

/// The reason a section has no entry, as the document holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Emptied {
    /// The `list-empty-reason` code the caller gave.
    Coded(EmptyReason),
    /// The `emptyReason` a mapping run wrote, kept whole.
    Mapped(CodeableConcept),
}

/// One section of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub(super) slot: &'static Slot,
    pub(super) paragraphs: Vec<String>,
    pub(super) entries: Vec<String>,
    pub(super) authors: Vec<String>,
    pub(super) empty: Option<Emptied>,
    pub(super) extensions: Vec<Extension>,
}

impl Section {
    /// Creates the section of `slot`, its generated narrative the
    /// `paragraphs`, with no entry and no author.
    #[must_use]
    pub fn new(slot: &'static Slot, paragraphs: Vec<String>) -> Self {
        Self {
            slot,
            paragraphs,
            entries: Vec::new(),
            authors: Vec::new(),
            empty: None,
            extensions: Vec::new(),
        }
    }

    /// Returns the slot.
    #[must_use]
    pub const fn slot(&self) -> &'static Slot {
        self.slot
    }

    /// Returns this section with the entry `full_url`, the `fullUrl` of a
    /// resource the document holds ([`super::Document::add`]).
    #[must_use]
    pub fn with_entry(mut self, full_url: String) -> Self {
        self.entries.push(full_url);
        self
    }

    /// Returns this section with the author `full_url`, the `fullUrl` of a
    /// resource the document holds.
    #[must_use]
    pub fn with_author(mut self, full_url: String) -> Self {
        self.authors.push(full_url);
        self
    }

    /// Returns this section with the extension `extension`, one of the
    /// extensions the form's `Composition` profile slices the section's
    /// `extension` by.
    #[must_use]
    pub fn with_extension(mut self, extension: Extension) -> Self {
        self.extensions.push(extension);
        self
    }

    /// Returns this section with one paragraph of its narrative for each
    /// path of `paths`, naming an element the source composition holds that
    /// the section's entries do not carry, by its openEHR path and never by
    /// its value (`eehrxf::mapping::left_out` reads them from a run).
    ///
    /// A reader of the section's narrative is then told the record holds
    /// more than the document shows. No specification governs where the
    /// declaration goes: our own design.
    #[must_use]
    pub fn with_left_out<I: IntoIterator<Item = String>>(mut self, paths: I) -> Self {
        self.paragraphs.extend(paths.into_iter().map(|path| {
            format!(
                "The source record holds the element {path}, which this document does not carry."
            )
        }));
        self
    }

    /// Returns this section with `reason` as the reason it has no entry,
    /// which the document writes only while it has none (`cmp-2`). A
    /// [`super::form::SectionRule::Reasoned`] rule of the document's form
    /// requires it of a section with no entry.
    #[must_use]
    pub fn empty_because(mut self, reason: EmptyReason) -> Self {
        self.empty = Some(Emptied::Coded(reason));
        self
    }
}

/// One section a mapping run wrote, placed in the document's form.
///
/// It holds the slot the section's LOINC code names, the `fullUrl` of each
/// entry it references, as [`super::Document::with_mapped_composition`]
/// added them, and the `emptyReason` the run wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedSection {
    /// The slot of the form the section's code names.
    pub slot: &'static Slot,
    /// The `fullUrl` of each entry, in the section's order.
    pub entries: Vec<String>,
    /// The `emptyReason` the run wrote on the section, when it wrote one.
    pub empty_reason: Option<CodeableConcept>,
}

impl MappedSection {
    /// Returns the document section of this mapped section, its generated
    /// narrative the `paragraphs`, holding every entry and the run's
    /// `emptyReason`.
    #[must_use]
    pub fn into_section(self, paragraphs: Vec<String>) -> Section {
        Section {
            entries: self.entries,
            empty: self.empty_reason.map(Emptied::Mapped),
            ..Section::new(self.slot, paragraphs)
        }
    }
}
