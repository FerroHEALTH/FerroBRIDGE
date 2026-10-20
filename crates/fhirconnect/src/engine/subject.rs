// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identifiers a run offers as the subject reference of its EHR.
//!
//! `EHR_STATUS` is its own versioned object, and its `subject` is a
//! `PARTY_SELF` whose `external_ref` "can be used to contain a direct
//! reference to the subject in a demographic or identity service" (openEHR
//! RM EHR IM Release 1.1.0, the `EHR_STATUS` class). No composition node holds
//! it, so a `mappingCode` function that writes it hands its identifiers to
//! the run's [`crate::engine::outcome::Outcome::subject`] and the caller that
//! commits decides which one the CDR's `EHR_STATUS` takes. No specification
//! defines this channel: our own design.

/// One FHIR `Identifier` a run offers as the subject of its EHR.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubjectIdentifier {
    system: Option<String>,
    value: String,
}

impl SubjectIdentifier {
    /// Pairs an `Identifier.value` with its `Identifier.system`.
    #[must_use]
    pub fn new(system: Option<String>, value: impl Into<String>) -> Self {
        Self {
            system,
            value: value.into(),
        }
    }

    /// Returns the namespace of the identifier, `Identifier.system`.
    #[must_use]
    pub fn system(&self) -> Option<&str> {
        self.system.as_deref()
    }

    /// Returns the identifier itself, `Identifier.value`.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}
