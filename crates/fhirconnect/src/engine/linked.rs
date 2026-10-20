// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the composition a `link` names was committed.
//!
//! "The engine checks the `meta.url` contained in the FHIR resource and
//! searches all available FHIRconnect context mappings. If a context mapping
//! for this `meta.url` is found, it is executed and saved as a separate
//! composition. This composition is linked using the openEHR links"
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/concept-type/concept-mappings.adoc`,
//! §Linked mappings). R4 defines no `meta.url`; the profiles a resource claims
//! are `meta.profile`, a list (<https://hl7.org/fhir/R4/resource.html#Meta>).
//!
//! The engine makes no call of its own, so the other context run and the
//! commit belong to the caller: a run that is handed a [`LinkedCompositions`]
//! resolves the resource a `link` names through the reference seam and asks
//! where its composition stands. The answer is a `DV_EHR_URI`, which is what
//! `LINK.target` holds. A run handed none refuses a link whose target is no
//! `ehr:` URI, since one run produces one composition. No specification
//! governs the shape of the seam: our own design.

use core::fmt;

use fhir_types::codec::Value;
use openehr_rm::v1_2::paths::EhrUri;

use crate::engine::seam::ReferenceError;

/// Where the composition each linked resource was committed as.
pub trait LinkedCompositions: fmt::Debug + Send + Sync {
    /// Returns where the composition of `resource`, which the literal
    /// `reference` names, stands.
    ///
    /// # Errors
    ///
    /// Returns [`ReferenceError`] when the caller could not run or commit the
    /// linked composition, which refuses the run and is never a skipped link.
    fn composition(&self, reference: &str, resource: &Value) -> Result<Linked, ReferenceError>;
}

/// What a caller answers for one linked resource.
#[derive(Debug, Clone, PartialEq)]
pub enum Linked {
    /// The linked composition stands at this `ehr:` URI.
    At(EhrUri),
    /// No context mapping maps the resource, so it has no composition and the
    /// link is a declared loss.
    Unmapped,
}
