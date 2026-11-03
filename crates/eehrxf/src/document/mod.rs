// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The documents of the priority categories in their HL7 Europe forms
//! (feature `fhir-r4` with `patient-summary`, `laboratory`, `discharge` or
//! `imaging`).
//!
//! A FHIR document is a `Bundle` of type `document` whose first entry is a
//! `Composition` and whose other entries are every resource the composition
//! references (<https://hl7.org/fhir/R4/documents.html>). A [`Document`]
//! collects the parts, names each entry by a `fullUrl` of the form
//! `{base}/{type}/{id}` under the base it is given, and writes the `Bundle`
//! of its [`Form`]: the profiles of one category's document, its
//! `Composition.type`, its sections, each with its LOINC code, its title, a
//! generated narrative, its entries and, when it has none, an empty reason,
//! and the report a form carries beside its composition. The patient summary
//! is written to HL7 Europe Patient Summary 1.0.0-ballot
//! ([`PATIENT_SUMMARY_FORM`]), the laboratory report to HL7 Europe
//! Laboratory Report 2.0.0 (`laboratory`), the discharge report to HL7
//! Europe Hospital Discharge Report 1.0.0 (`discharge`) and the imaging report
//! to HL7 Europe Imaging Report 1.0.0-ballot (`imaging`). What feeds each
//! section is the caller's: this module decides no clinical content.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use fhir_types::codec::{Json, Object, Path, Value};
use fhir_types::r4::bundle::{Bundle, BundleEntry};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::composition::{Composition, CompositionSection};
use fhir_types::r4::diagnostic_report::DiagnosticReport;
use fhir_types::r4::encounter::Encounter;
use fhir_types::r4::extension::{Extension, ExtensionValue};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::meta::Meta;
use fhir_types::r4::narrative::Narrative;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;

mod assembly;
#[cfg(feature = "discharge")]
pub mod discharge;
pub mod form;
#[cfg(feature = "imaging")]
pub mod imaging;
#[cfg(feature = "laboratory")]
pub mod laboratory;
mod mapped;
pub mod section;

#[cfg(feature = "patient-summary")]
use form::PATIENT_SUMMARY_FORM;
use form::{Code, Form};
use section::Section;

/// The `id` a report that carries none is written under.
const REPORT_ID: &str = "report";

/// The canonical URL of the HL7 Europe Patient Summary `Bundle` profile.
#[cfg(feature = "patient-summary")]
pub const EPS_BUNDLE: &str = "http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps";

/// The canonical URL of the HL7 Europe Patient Summary `Patient` profile.
#[cfg(feature = "patient-summary")]
pub const EPS_PATIENT: &str = "http://hl7.eu/fhir/eps/StructureDefinition/patient-eu-eps";

/// The LOINC code system, which codes the document type and every section.
pub const LOINC: &str = "http://loinc.org";

/// The LOINC code the `composition-eu-eps` profile fixes as
/// `Composition.type`, a patient summary document.
#[cfg(feature = "patient-summary")]
pub const PATIENT_SUMMARY: &str = "60591-5";

/// The code system of a section's empty reason, FHIR R4 `list-empty-reason`
/// (<https://hl7.org/fhir/R4/valueset-list-empty-reason.html>).
pub const EMPTY_REASON: &str = "http://terminology.hl7.org/CodeSystem/list-empty-reason";

/// The identifier system of a document identifier written as a URN, RFC 3986
/// (<https://hl7.org/fhir/R4/identifier-registry.html>).
pub const URN_SYSTEM: &str = "urn:ietf:rfc:3986";

/// The canonical URL of the HL7 Europe base `Patient` profile, the subject
/// of the laboratory, discharge and imaging reports.
#[cfg(any(feature = "laboratory", feature = "discharge", feature = "imaging"))]
pub const EU_CORE_PATIENT: &str = "http://hl7.eu/fhir/base/StructureDefinition/patient-eu-core";

/// Why a document cannot be written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DocumentError {
    /// A resource carries no `id`, so it has no `fullUrl`.
    #[error("a {resource} resource carries no id")]
    NoId {
        /// The resource type.
        resource: String,
    },
    /// A resource could not be encoded or decoded as FHIR JSON.
    #[error("a resource could not be encoded as FHIR JSON")]
    Codec(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// A section names an entry or an author the document does not hold.
    #[error("the section {slice} references a resource the document does not hold")]
    Dangling {
        /// The section slice.
        slice: &'static str,
    },
    /// Two sections share one slice.
    #[error("the section {slice} is given twice")]
    Repeated {
        /// The section slice.
        slice: &'static str,
    },
    /// A section is none of the form's.
    #[error("the section {slice} is not a section of the document's form")]
    Foreign {
        /// The section slice.
        slice: &'static str,
    },
    /// The form requires a part the document was not given.
    #[error("the document's form requires its {part}, which was not given")]
    Missing {
        /// The part: `encounter` or `language`.
        part: &'static str,
    },
    /// The form holds a report beside its composition and none was given,
    /// or it holds none and one was.
    #[error("the document's form and its report disagree: the form {expected} one")]
    Report {
        /// What the form expects: `requires`, `holds no` or `takes one`.
        expected: &'static str,
    },
    /// A mapping run whose report the document takes holds no
    /// `DiagnosticReport` or more than one.
    #[error("the mapping run holds {found} DiagnosticReport resources, not one")]
    Unmapped {
        /// How many the run holds.
        found: usize,
    },
    /// The report's `fullUrl` is an entry's already, which FHIR R4 `bdl-7`
    /// forbids.
    #[error("the report's fullUrl is an entry's already")]
    Collision {
        /// The `fullUrl` both would take.
        full_url: String,
    },
}

/// A document as it is assembled.
#[derive(Debug, Clone)]
pub struct Document {
    form: &'static Form,
    base: String,
    identifier: String,
    timestamp: String,
    title: String,
    patient: Option<String>,
    authors: Vec<String>,
    entries: Vec<(String, Resource)>,
    sections: Vec<Section>,
    report: Option<DiagnosticReport>,
    encounter: Option<String>,
    language: Option<String>,
}

impl Document {
    /// Creates the patient summary named `identifier`, a URN, written at
    /// `timestamp`, a FHIR `instant`, titled `title`, its entries named under
    /// the absolute FHIR base `base`.
    #[cfg(feature = "patient-summary")]
    #[must_use]
    pub fn new(base: &str, identifier: String, timestamp: String, title: String) -> Self {
        Self::of(&PATIENT_SUMMARY_FORM, base, identifier, timestamp, title)
    }

    /// Creates the document of `form` named `identifier`, a URN, written at
    /// `timestamp`, a FHIR `instant`, titled `title`, its entries named under
    /// the absolute FHIR base `base`.
    #[must_use]
    pub fn of(
        form: &'static Form,
        base: &str,
        identifier: String,
        timestamp: String,
        title: String,
    ) -> Self {
        Self {
            form,
            base: base.trim_end_matches('/').to_owned(),
            identifier,
            timestamp,
            title,
            patient: None,
            authors: Vec::new(),
            entries: Vec::new(),
            sections: Vec::new(),
            report: None,
            encounter: None,
            language: None,
        }
    }

    /// Returns the document's form.
    #[must_use]
    pub const fn form(&self) -> &'static Form {
        self.form
    }

    /// Returns the `fullUrl` of the resource `resource_type`/`id`
    /// under the document's base.
    #[must_use]
    pub fn full_url(&self, resource_type: &str, id: &str) -> String {
        format!("{}/{resource_type}/{id}", self.base)
    }

    /// Adds `patient`, under its `id`, as the document's subject, claiming
    /// the form's `Patient` profile, and returns the relative reference
    /// `Patient/{id}` every reference to the patient is written with.
    ///
    /// The document's `Composition.subject` is written with that reference,
    /// and a mapping run names the patient with it too, so the document
    /// names its subject in one form, which resolves to the patient's entry
    /// against the `fullUrl` base of the entry that holds it.
    ///
    /// # Errors
    ///
    /// [`DocumentError::NoId`] for a patient with no `id`, and
    /// [`DocumentError::Codec`] when it cannot be encoded.
    pub fn with_patient(&mut self, mut patient: Patient) -> Result<String, DocumentError> {
        let meta = patient.meta.get_or_insert_with(Meta::default);
        meta.profile.push(self.form.patient.into());
        self.add(&[Resource::Patient(Box::new(patient))])?;
        let id = match self.entries.last() {
            Some((_, Resource::Patient(added))) => added.id.clone(),
            _ => None,
        }
        .ok_or_else(|| DocumentError::NoId {
            resource: String::from("Patient"),
        })?;
        // NOTE: FHIR R4 Bundle references (bundle.html#references) resolve `Patient/{id}` against the
        // holding entry's RESTful fullUrl base; writing the one relative form keeps one subject string.
        let reference = format!("Patient/{id}");
        self.patient = Some(reference.clone());
        Ok(reference)
    }

    /// Adds `authored` and names it an author of the composition, returning
    /// its `fullUrl`.
    ///
    /// # Errors
    ///
    /// The errors of [`Document::add`].
    pub fn with_author(&mut self, authored: &Resource) -> Result<String, DocumentError> {
        let full_url = self
            .add(std::slice::from_ref(authored))?
            .into_iter()
            .next()
            .ok_or_else(|| DocumentError::NoId {
                resource: String::from("author"),
            })?;
        self.authors.push(full_url.clone());
        Ok(full_url)
    }

    /// Adds `section`.
    ///
    /// # Errors
    ///
    /// [`DocumentError::Foreign`] for a section that is none of the form's,
    /// and [`DocumentError::Repeated`] for a slice the document already
    /// holds.
    pub fn with_section(&mut self, section: Section) -> Result<(), DocumentError> {
        if !self.form.slots.contains(section.slot) {
            return Err(DocumentError::Foreign {
                slice: section.slot.slice,
            });
        }
        if self
            .sections
            .iter()
            .any(|held| held.slot.slice == section.slot.slice)
        {
            return Err(DocumentError::Repeated {
                slice: section.slot.slice,
            });
        }
        self.sections.push(section);
        Ok(())
    }

    /// Adds `encounter`, under its `id`, as the encounter the document is
    /// about, claiming the form's `Encounter` profile when it names one, and
    /// returns the relative reference `Encounter/{id}` the composition, and
    /// a report, name it with (`dr-comp-enc` holds the two equal).
    ///
    /// # Errors
    ///
    /// [`DocumentError::NoId`] for an encounter with no `id`, and
    /// [`DocumentError::Codec`] when it cannot be encoded.
    pub fn with_encounter(&mut self, mut encounter: Encounter) -> Result<String, DocumentError> {
        if let Some(profile) = self.form.encounter {
            let meta = encounter.meta.get_or_insert_with(Meta::default);
            meta.profile.push(profile.into());
        }
        self.add(&[Resource::Encounter(Box::new(encounter))])?;
        let id = match self.entries.last() {
            Some((_, Resource::Encounter(added))) => added.id.clone(),
            _ => None,
        }
        .ok_or_else(|| DocumentError::NoId {
            resource: String::from("Encounter"),
        })?;
        let reference = format!("Encounter/{id}");
        self.encounter = Some(reference.clone());
        Ok(reference)
    }

    /// Sets `language`, a BCP 47 tag, as the document's `Bundle.language`.
    pub fn with_language(&mut self, language: String) {
        self.language = Some(language);
    }

    /// Sets `report` as the report the form holds beside the composition.
    ///
    /// The document writes it as the `Bundle`'s second entry, claiming the
    /// form's report profile, with the document's identifier, its
    /// `Composition.type` among the report's codings, the document's subject,
    /// and an extension naming the composition, and the composition names it
    /// in turn: the `Bundle-eu-lab` invariants `dr-comp-identifier`,
    /// `dr-comp-type` and `dr-comp-subj` hold the two to one another. A
    /// category the report carries is written on the composition too
    /// (`dr-comp-category`).
    ///
    /// The report takes the `fullUrl` `DiagnosticReport/{id}` under the
    /// document's base, which no entry may hold (FHIR R4 `bdl-7`); a
    /// resource [`Document::add`] adds later under it is renamed as a
    /// repeated `Type/id` is.
    ///
    /// # Errors
    ///
    /// [`DocumentError::Report`] when the form holds no report, and
    /// [`DocumentError::Collision`] when an entry holds the report's
    /// `fullUrl` already.
    pub fn with_report(&mut self, report: DiagnosticReport) -> Result<(), DocumentError> {
        if self.form.report.is_none() {
            return Err(DocumentError::Report {
                expected: "holds no",
            });
        }
        let full_url = self.report_url(&report);
        if self.entries.iter().any(|(url, _)| *url == full_url) {
            return Err(DocumentError::Collision { full_url });
        }
        self.report = Some(report);
        Ok(())
    }

    /// Returns the `fullUrl` the document writes `report` under.
    fn report_url(&self, report: &DiagnosticReport) -> String {
        self.full_url(
            "DiagnosticReport",
            report.id.as_deref().unwrap_or(REPORT_ID),
        )
    }

    /// Adds `resources`, the output of one mapping run that reference one
    /// another by `Type/id`, and returns the `fullUrl` of each, in order.
    ///
    /// A resource with no `id` takes the next free `{type}-{n}` id, and one
    /// whose `Type/id` the document already holds takes `{id}-{n}`, every
    /// reference among `resources` to the renamed one following it, so two
    /// runs over the same composition never share a `fullUrl` and none is
    /// merged into the other (FHIR R4 `bdl-7`).
    ///
    /// # Errors
    ///
    /// [`DocumentError::Codec`] when a resource cannot be encoded or read
    /// back.
    pub fn add(&mut self, resources: &[Resource]) -> Result<Vec<String>, DocumentError> {
        let mut objects = Vec::with_capacity(resources.len());
        for resource in resources {
            objects.push(Json::to_json(resource).map_err(codec)?);
        }
        let mut taken: BTreeSet<String> = self.entries.iter().map(|(url, _)| url.clone()).collect();
        if let Some(ref report) = self.report {
            taken.insert(self.report_url(report));
        }
        let mut renamed: Vec<(String, String)> = Vec::new();
        let mut keys = Vec::with_capacity(objects.len());
        for object in &mut objects {
            let kind = text(object, "resourceType")
                .unwrap_or("Resource")
                .to_owned();
            let held = text(object, "id").map(str::to_owned);
            let stem = held.clone().unwrap_or_else(|| kind.to_ascii_lowercase());
            let id = free(
                &mut taken,
                |id| self.full_url(&kind, id),
                &stem,
                held.is_some(),
            );
            if let Some(held) = held.filter(|held| *held != id) {
                renamed.push((format!("{kind}/{held}"), format!("{kind}/{id}")));
            }
            object.insert(String::from("id"), Value::String(id.clone()));
            keys.push(self.full_url(&kind, &id));
        }
        let mut added = Vec::with_capacity(objects.len());
        for (mut object, full_url) in objects.into_iter().zip(keys) {
            if !renamed.is_empty() {
                for value in object.values_mut() {
                    rename(value, &renamed);
                }
            }
            let resource =
                Resource::from_json(&object, &mut Path::root("Resource")).map_err(codec)?;
            self.entries.push((full_url.clone(), resource));
            added.push(full_url);
        }
        Ok(added)
    }

    /// Writes the document `Bundle`: the composition first, then every
    /// resource in the order it was added.
    ///
    /// The composition is `final`, of the form's type, about the patient,
    /// by every author, with each section in the order it was added. A
    /// section with no entry carries its empty reason. The report of a form
    /// that holds one follows the composition ([`Document::with_report`]).
    ///
    /// # Errors
    ///
    /// [`DocumentError::Dangling`] when a section names a `fullUrl` the
    /// document does not hold, or no patient was added, and
    /// [`DocumentError::Report`] when the form holds a report and none was
    /// given.
    pub fn into_bundle(mut self) -> Result<Bundle, DocumentError> {
        let held: BTreeSet<&str> = self.entries.iter().map(|(url, _)| url.as_str()).collect();
        let mut sections = Vec::with_capacity(self.sections.len());
        for section in &self.sections {
            if section
                .entries
                .iter()
                .chain(&section.authors)
                .any(|url| !held.contains(url.as_str()))
            {
                return Err(DocumentError::Dangling {
                    slice: section.slot.slice,
                });
            }
            sections.push(composition_section(section));
        }
        let subject = self
            .patient
            .clone()
            .ok_or(DocumentError::Dangling { slice: "subject" })?;
        let id = self
            .identifier
            .rsplit(':')
            .next()
            .unwrap_or("composition")
            .to_owned();
        let composition_url = self.full_url("Composition", &id);
        if self.form.encounter.is_some() && self.encounter.is_none() {
            return Err(DocumentError::Missing { part: "encounter" });
        }
        if self.form.language && self.language.is_none() {
            return Err(DocumentError::Missing { part: "language" });
        }
        let report = self.report_written(&subject, &composition_url)?;
        let mut composition = Composition {
            id: Some(id),
            identifier: Some(Identifier {
                system: Some(URN_SYSTEM.into()),
                value: Some(self.identifier.as_str().into()),
                ..Identifier::default()
            }),
            status: "final".into(),
            r#type: CodeableConcept::default(),
            subject: Some(reference(&subject)),
            encounter: self.encounter.as_deref().map(reference),
            date: self.timestamp.as_str().into(),
            author: self
                .authors
                .iter()
                .map(|author| reference(author))
                .collect(),
            title: self.title.as_str().into(),
            section: sections,
            ..Composition::default()
        };
        if let (Some(companion), Some((report_url, _))) = (self.form.report, &report) {
            composition.extension.push(Extension {
                url: companion.link.into(),
                value: Some(ExtensionValue::Reference(Box::new(reference(report_url)))),
                ..Extension::default()
            });
        }
        self.arrange(&mut composition, report.as_ref().map(|(_, report)| report));
        let mut entry = vec![BundleEntry {
            full_url: Some(composition_url.into()),
            resource: Some(Resource::Composition(Box::new(composition))),
            ..BundleEntry::default()
        }];
        entry.extend(report.map(|(full_url, report)| BundleEntry {
            full_url: Some(full_url.into()),
            resource: Some(Resource::DiagnosticReport(Box::new(report))),
            ..BundleEntry::default()
        }));
        entry.extend(
            self.entries
                .into_iter()
                .map(|(full_url, resource)| BundleEntry {
                    full_url: Some(full_url.into()),
                    resource: Some(resource),
                    ..BundleEntry::default()
                }),
        );
        Ok(Bundle {
            meta: Some(Meta {
                profile: vec![self.form.bundle.into()],
                ..Meta::default()
            }),
            identifier: Some(Identifier {
                system: Some(URN_SYSTEM.into()),
                value: Some(self.identifier.into()),
                ..Identifier::default()
            }),
            language: self.language.map(Into::into),
            r#type: "document".into(),
            timestamp: Some(self.timestamp.into()),
            entry,
            ..Bundle::default()
        })
    }
}

/// The composition section `section` writes.
fn composition_section(section: &Section) -> CompositionSection {
    CompositionSection {
        title: Some(section.slot.title.into()),
        code: Some(CodeableConcept {
            coding: vec![loinc(section.slot.code)],
            ..CodeableConcept::default()
        }),
        author: section.authors.iter().map(|url| reference(url)).collect(),
        text: Some(narrative(section.slot.title, &section.paragraphs)),
        entry: section.entries.iter().map(|url| reference(url)).collect(),
        // NOTE: FHIR R4 Composition cmp-2: an empty reason stands only beside no entry.
        empty_reason: section
            .empty
            .filter(|_| section.entries.is_empty())
            .map(|reason| CodeableConcept {
                coding: vec![Coding {
                    system: Some(EMPTY_REASON.into()),
                    code: Some(reason.code().into()),
                    ..Coding::default()
                }],
                ..CodeableConcept::default()
            }),
        ..CompositionSection::default()
    }
}

/// The generated narrative headed `title`, one XHTML paragraph per entry of
/// `paragraphs`, every character escaped (FHIR R4 Narrative, `txt-1`).
#[must_use]
pub fn narrative(title: &str, paragraphs: &[String]) -> Narrative {
    let mut div = String::from(r#"<div xmlns="http://www.w3.org/1999/xhtml">"#);
    // NOTE: writing to a String cannot fail, so each result is dropped.
    let _written: std::fmt::Result = write!(div, "<h3>{}</h3>", escaped(title));
    for paragraph in paragraphs {
        let _written: std::fmt::Result = write!(div, "<p>{}</p>", escaped(paragraph));
    }
    div.push_str("</div>");
    Narrative {
        status: "generated".into(),
        div: div.into(),
        ..Narrative::default()
    }
}

/// `text` with the five XML special characters escaped.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// The LOINC coding of `code`.
fn loinc(code: &'static str) -> Coding {
    coding(Code {
        system: LOINC,
        code,
    })
}

/// The coding of `code`.
fn coding(code: Code) -> Coding {
    Coding {
        system: Some(code.system.into()),
        code: Some(code.code.into()),
        ..Coding::default()
    }
}

/// The literal reference `url`.
fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}

/// The string member `key` of `object`.
fn text<'a>(object: &'a Object, key: &str) -> Option<&'a str> {
    match object.get(key) {
        Some(Value::String(text)) => Some(text),
        _ => None,
    }
}

/// The first id from `stem` whose `fullUrl`, as `url` writes it, is not in
/// `taken`, which it is then added to: `stem` itself when `held` names it,
/// and `{stem}-{n}` otherwise.
fn free(
    taken: &mut BTreeSet<String>,
    url: impl Fn(&str) -> String,
    stem: &str,
    held: bool,
) -> String {
    let mut candidate = if held {
        stem.to_owned()
    } else {
        format!("{stem}-1")
    };
    let mut next = 1_u32;
    while taken.contains(&url(&candidate)) {
        next = next.saturating_add(1);
        candidate = format!("{stem}-{next}");
    }
    taken.insert(url(&candidate));
    candidate
}

/// Rewrites every `reference` member under `value` that names a renamed
/// `Type/id`, by the pairs `renamed`.
fn rename(value: &mut Value, renamed: &[(String, String)]) {
    match value {
        Value::Object(object) => {
            for (key, member) in object.iter_mut() {
                if let (true, Value::String(text)) = (key == "reference", &mut *member)
                    && let Some((_, to)) = renamed.iter().find(|(from, _)| from == text)
                {
                    to.clone_into(text);
                } else {
                    rename(member, renamed);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                rename(item, renamed);
            }
        }
        _ => {}
    }
}

/// The codec failure `source`.
fn codec(source: impl std::error::Error + Send + Sync + 'static) -> DocumentError {
    DocumentError::Codec(Box::new(source))
}
