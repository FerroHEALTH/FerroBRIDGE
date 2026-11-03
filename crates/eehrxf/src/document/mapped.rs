// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How a [`Document`] takes the report a mapping run wrote (no
//! specification governs the split: our own design).

use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::resource::Resource;

use crate::document::{Document, DocumentError};

impl Document {
    /// Adds the resources of `run`, the `Bundle` one mapping run answers,
    /// with its one `DiagnosticReport` as the report the form holds beside
    /// the composition, and returns the `fullUrl` of each result the report
    /// names, in its order, the entries of the section that lists them.
    ///
    /// The run's `OperationOutcome`, the losses it declared, reports the run
    /// and stays out of the document; read them from the run itself.
    /// Every other resource of the run, its `Provenance` included, is added
    /// as [`Document::add`] adds the output of one run, and the report takes
    /// the same renaming, so its references keep naming the run's results.
    /// The report is then set as [`Document::with_report`] sets one.
    ///
    /// # Errors
    ///
    /// [`DocumentError::Report`] when the form holds no report or already
    /// holds one, [`DocumentError::Unmapped`] when the run holds no
    /// `DiagnosticReport` or more than one, and the errors of
    /// [`Document::add`].
    pub fn with_mapped_report(&mut self, run: &Bundle) -> Result<Vec<String>, DocumentError> {
        if self.form.report.is_none() {
            return Err(DocumentError::Report {
                expected: "holds no",
            });
        }
        if self.report.is_some() {
            return Err(DocumentError::Report {
                expected: "takes one",
            });
        }
        // NOTE: FHIR R4 documents.html: every resource of a document is reached from its
        // Composition; the run's OperationOutcome reports the run and is no document content.
        let resources: Vec<Resource> = run
            .entry
            .iter()
            .filter_map(|entry| entry.resource.clone())
            .filter(|resource| !matches!(resource, Resource::OperationOutcome(_)))
            .collect();
        let found = resources
            .iter()
            .filter(|resource| matches!(resource, Resource::DiagnosticReport(_)))
            .count();
        if found != 1 {
            return Err(DocumentError::Unmapped { found });
        }
        let first = self.entries.len();
        self.add(&resources)?;
        let position = self
            .entries
            .iter()
            .skip(first)
            .position(|(_, resource)| matches!(resource, Resource::DiagnosticReport(_)))
            .and_then(|offset| first.checked_add(offset))
            .ok_or(DocumentError::Unmapped { found: 0 })?;
        let (_, Resource::DiagnosticReport(report)) = self.entries.remove(position) else {
            return Err(DocumentError::Unmapped { found: 0 });
        };
        let results = report
            .result
            .iter()
            .filter_map(|result| result.reference.as_ref())
            .filter_map(|reference| reference.value.as_deref())
            // NOTE: FHIR R4 Bundle references (bundle.html#references): a relative reference resolves
            // against the entry's RESTful base, and one with a scheme is its own fullUrl.
            .map(|reference| {
                if reference.contains(':') {
                    reference.to_owned()
                } else {
                    format!("{}/{reference}", self.base)
                }
            })
            .collect();
        self.report = Some(*report);
        Ok(results)
    }
}
