// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One section of a document: its slot, its narrative, its entries and the
//! reason it has none.

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

/// One section of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub(super) slot: &'static Slot,
    pub(super) paragraphs: Vec<String>,
    pub(super) entries: Vec<String>,
    pub(super) authors: Vec<String>,
    pub(super) empty: Option<EmptyReason>,
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

    /// Returns this section with `reason` as the reason it has no entry,
    /// which the document writes only while it has none (`cmp-2`). A
    /// [`super::form::SectionRule::Reasoned`] rule of the document's form
    /// requires it of a section with no entry.
    #[must_use]
    pub const fn empty_because(mut self, reason: EmptyReason) -> Self {
        self.empty = Some(reason);
        self
    }
}

/// One section a mapping run wrote, placed in the document's form.
///
/// It holds the slot the section's LOINC code names and the `fullUrl` of
/// each entry it references, as
/// [`super::Document::with_mapped_composition`] added them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedSection {
    /// The slot of the form the section's code names.
    pub slot: &'static Slot,
    /// The `fullUrl` of each entry, in the section's order.
    pub entries: Vec<String>,
}

impl MappedSection {
    /// Returns the document section of this mapped section, its generated
    /// narrative the `paragraphs`, holding every entry.
    #[must_use]
    pub fn into_section(self, paragraphs: Vec<String>) -> Section {
        Section {
            entries: self.entries,
            ..Section::new(self.slot, paragraphs)
        }
    }
}
