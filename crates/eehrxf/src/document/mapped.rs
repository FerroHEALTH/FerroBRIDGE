// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How a [`Document`] takes the report or the composition a mapping run
//! wrote (no specification governs the split: our own design).

use fhir_types::codec::Json;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::meta::Meta;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;

use crate::document::section::MappedSection;
use crate::document::{Document, DocumentError, LOINC, codec, text};

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

impl Document {
    /// Adds the resources of `run`, the `Bundle` one mapping run answers,
    /// whose context wrote the document's `Composition`, and returns each
    /// section that composition holds, placed in the form by its code.
    ///
    /// The run's `Composition` stays out of the document, which writes its
    /// own from the form ([`Document::into_bundle`]); what the document
    /// takes from it is the encounter it names and its sections. Every other
    /// resource of the run, its `Provenance` included, is added as
    /// [`Document::add`] adds the output of one run, and its
    /// `OperationOutcome`, the losses the run declared, stays out as
    /// [`Document::with_mapped_report`] leaves it. When the form takes an
    /// encounter, the one the composition names becomes the document's and
    /// claims the form's `Encounter` profile, as [`Document::with_encounter`]
    /// makes it. Each section is placed in the form's slot whose LOINC code
    /// a coding of the section's `code` carries, and its entries name the
    /// `fullUrl`s the run's resources were added under.
    ///
    /// # Errors
    ///
    /// [`DocumentError::Uncomposed`] when the run holds no `Composition` or
    /// more than one, [`DocumentError::Unslotted`] for a section whose code
    /// names no slot of the form, [`DocumentError::Dangling`] for an
    /// encounter or an entry the run does not hold, and the errors of
    /// [`Document::add`].
    pub fn with_mapped_composition(
        &mut self,
        run: &Bundle,
    ) -> Result<Vec<MappedSection>, DocumentError> {
        let mut compositions = Vec::new();
        let mut resources = Vec::new();
        for resource in run.entry.iter().filter_map(|entry| entry.resource.clone()) {
            match resource {
                Resource::Composition(composition) => compositions.push(composition),
                // NOTE: FHIR R4 documents.html: every resource of a document is reached from its
                // Composition; the run's OperationOutcome reports the run and is no document content.
                Resource::OperationOutcome(_) => {}
                other => resources.push(other),
            }
        }
        let [composition] = compositions.as_slice() else {
            return Err(DocumentError::Uncomposed {
                found: compositions.len(),
            });
        };
        let keys = resources
            .iter()
            .map(|resource| {
                let object = Json::to_json(resource).map_err(codec)?;
                Ok(match (text(&object, "resourceType"), text(&object, "id")) {
                    (Some(kind), Some(id)) => Some(format!("{kind}/{id}")),
                    _ => None,
                })
            })
            .collect::<Result<Vec<_>, DocumentError>>()?;
        let urls = self.add(&resources)?;
        let added = |reference: &Reference| -> Option<String> {
            let named = reference.reference.as_ref()?.value.as_deref()?;
            keys.iter()
                .zip(&urls)
                .find(|(key, url)| key.as_deref() == Some(named) || url.as_str() == named)
                .map(|(_, url)| url.clone())
        };
        if let Some(profile) = self.form.encounter
            && let Some(named) = composition.encounter.as_ref()
        {
            let url = added(named).ok_or(DocumentError::Dangling { slice: "encounter" })?;
            self.claim_encounter(&url, profile)?;
        }
        let mut sections = Vec::with_capacity(composition.section.len());
        for section in &composition.section {
            let codes: Vec<&str> = section
                .code
                .iter()
                .flat_map(|code| &code.coding)
                .filter(|coding| {
                    coding
                        .system
                        .as_ref()
                        .and_then(|system| system.value.as_deref())
                        == Some(LOINC)
                })
                .filter_map(|coding| coding.code.as_ref().and_then(|code| code.value.as_deref()))
                .collect();
            let slot = self
                .form
                .slots
                .iter()
                .find(|slot| codes.contains(&slot.code))
                .ok_or_else(|| DocumentError::Unslotted {
                    codes: codes.iter().map(|code| (*code).to_owned()).collect(),
                })?;
            let entries = section
                .entry
                .iter()
                .map(|entry| added(entry).ok_or(DocumentError::Dangling { slice: slot.slice }))
                .collect::<Result<Vec<_>, _>>()?;
            sections.push(MappedSection { slot, entries });
        }
        Ok(sections)
    }

    /// Sets the entry at `url`, an `Encounter`, as the encounter the
    /// document is about, claiming `profile` unless it claims it already.
    fn claim_encounter(&mut self, url: &str, profile: &str) -> Result<(), DocumentError> {
        let Some((_, Resource::Encounter(encounter))) =
            self.entries.iter_mut().find(|(held, _)| held == url)
        else {
            return Err(DocumentError::Dangling { slice: "encounter" });
        };
        let meta = encounter.meta.get_or_insert_with(Meta::default);
        if !meta
            .profile
            .iter()
            .any(|claimed| claimed.value.as_deref() == Some(profile))
        {
            meta.profile.push(profile.into());
        }
        let id = encounter.id.clone().ok_or_else(|| DocumentError::NoId {
            resource: String::from("Encounter"),
        })?;
        self.encounter = Some(format!("Encounter/{id}"));
        Ok(())
    }
}
