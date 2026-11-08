// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic example composition of a template, so a mapping can be tried
//! without hand-written input.
//!
//! The example is the output form `openehr-sdt` generates from the Web
//! Template (`openehr_sdt::flat::example`), validated against the template
//! through [`WebTemplateIndex::accept`], so an example is one the CDR's own
//! validation admits.

use openehr_sdt::flat::error::FlatError;
use openehr_sdt::flat::example::{DetailLevel, apply_output_uid, example_composition};

use crate::composition::CanonicalComposition;
use crate::index::WebTemplateIndex;
use crate::template::PathError;

impl WebTemplateIndex {
    /// Builds a synthetic example composition of this template.
    ///
    /// The example is the fully populated single-instance document
    /// ([`DetailLevel::Medium`]), or the mandatory skeleton
    /// ([`DetailLevel::Required`]) when that one does not build or validate.
    /// It carries the deterministic `uid` of the output form, which `$tofhir`
    /// reads as the composition's id, and no current time, so the same
    /// template gives the same example at every build.
    ///
    /// # Errors
    ///
    /// Returns [`PathError::NotComposition`] for a template whose root is no
    /// `COMPOSITION`, [`PathError::CompositionBuild`] when no example of
    /// either level builds and [`PathError::InvalidComposition`] when none
    /// validates; both answer the medium example's refusal.
    pub fn example_composition(&self) -> Result<CanonicalComposition, PathError> {
        self.example_at(DetailLevel::Medium).or_else(|medium| {
            self.example_at(DetailLevel::Required)
                .map_err(|_required| medium)
        })
    }

    /// Returns the validated example of `level`.
    fn example_at(&self, level: DetailLevel) -> Result<CanonicalComposition, PathError> {
        let mut value =
            example_composition(self.web_template(), level).map_err(|source| match source {
                FlatError::NoExampleForRoot { .. } => PathError::NotComposition {
                    template_id: self.template_id().to_owned(),
                },
                source => PathError::CompositionBuild {
                    template_id: self.template_id().to_owned(),
                    source: Box::new(source),
                },
            })?;
        apply_output_uid(&mut value, self.template_id());
        self.accept(value)
    }
}
