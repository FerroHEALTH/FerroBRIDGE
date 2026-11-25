// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document and data-group metadata of draft `EEHRxF` Annex III.
//!
//! Art 4 and Annex III of the draft `EEHRxF` implementing act (initiative
//! 17512, 2026-11-23) put the metadata of Table 1 on every exchanged
//! document and the metadata of Table 2 on every data group. A
//! [`DocumentMetadata`] holds what Table 1 asks beyond the parts a
//! [`Document`] already takes (subject, author, date, type, title,
//! custodian); [`Document::with_metadata`] writes it on the `Composition`,
//! and [`DocumentMetadata::describe`] writes the `DocumentReference` that
//! describes the written document, the shape MyHealth@EU
//! DocumentReference-minimal (`myhealth.eu.fhir.ncp-api` 9.1.0) constrains;
//! [`DocumentMetadata::describe_at`] writes it from a [`Header`] before the
//! document is written.
//!
//! | Table 1 element | `Composition` (R4) | `DocumentReference` (R4) |
//! |---|---|---|
//! | subject | `subject` | `subject` |
//! | identifier | `identifier`, and `Bundle.identifier` | `masterIdentifier` (the `Bundle`), `identifier` (the `Composition`) |
//! | version | the R5 `Composition.version` cross-version extension | the R5 `DocumentReference.version` cross-version extension |
//! | author\[x\] | `author` | `author` |
//! | date | `date` | `date` |
//! | attestation | `attester`, mode `professional` | none in R4 |
//! | status | `status` | `docStatus` |
//! | language | `language` | `content.attachment.language` |
//! | documentCategory | `category`, and the report's `category` on a laboratory or test report; not on an imaging report, whose form fixes it | `category` |
//! | documentType | `type` | `type` |
//! | documentTitle | `title` | `description` |
//! | period | `event.period` | `context.period` |
//! | legalAuthentication | `attester`, mode `legal` | `authenticator` |
//! | eventCategory | `event.code` | `context.event` |
//! | serviceSpecialty | none in R4 | `context.practiceSetting` |
//! | custodian | `custodian` | `custodian` |
//!
//! R4 holds one `Composition.language` and one
//! `DocumentReference.context.practiceSetting` where Table 1 allows many, so
//! a second value is refused with [`DocumentError::TooMany`] rather than
//! dropped.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use fhir_types::r4::attachment::Attachment;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::composition::{Composition, CompositionAttester, CompositionEvent};
use fhir_types::r4::diagnostic_report::DiagnosticReport;
use fhir_types::r4::document_reference::{
    DocumentReference, DocumentReferenceContent, DocumentReferenceContext,
};
use fhir_types::r4::extension::{Extension, ExtensionValue};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::meta::Meta;
use fhir_types::r4::period::Period;
use fhir_types::r4::primitives::{Code, DateTime};
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use sha1::{Digest as _, Sha1};

use crate::document::representation::PdfA;
use crate::document::{Document, DocumentError, reference};

/// The cross-version extension that carries R5 `Composition.version` in R4
/// (<https://hl7.org/fhir/extensions/xver.html>).
pub const COMPOSITION_VERSION: &str =
    "http://hl7.org/fhir/5.0/StructureDefinition/extension-Composition.version";

/// The cross-version extension that carries R5 `DocumentReference.version`
/// in R4 (<https://hl7.org/fhir/extensions/xver.html>).
pub const DOCUMENT_REFERENCE_VERSION: &str =
    "http://hl7.org/fhir/5.0/StructureDefinition/extension-DocumentReference.version";

/// The media type of a PDF document (RFC 8118).
pub const PDF: &str = "application/pdf";

/// The media type of a FHIR document in JSON (FHIR R4 `http.html#mime-type`).
pub const FHIR_JSON: &str = "application/fhir+json";

/// One attestation or legal authentication: who and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
    party: String,
    time: String,
}

impl Attestation {
    /// Creates the attestation of `party`, a reference to a document entry,
    /// at `time`, a FHIR `dateTime`.
    #[must_use]
    pub const fn new(party: String, time: String) -> Self {
        Self { party, time }
    }
}

/// The Annex III Table 1 metadata a [`Document`] does not take as parts.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentMetadata {
    version: String,
    category: CodeableConcept,
    status: String,
    languages: Vec<String>,
    period: Option<Period>,
    attestations: Vec<Attestation>,
    legal: Vec<Attestation>,
    events: Vec<CodeableConcept>,
    specialties: Vec<CodeableConcept>,
    source: Option<String>,
}

impl DocumentMetadata {
    /// Creates the metadata of a document at `version` in the document
    /// category `category` (LOINC, Table 1 `documentCategory`), keeping the
    /// status the document writes.
    #[must_use]
    pub const fn new(version: String, category: CodeableConcept) -> Self {
        Self {
            version,
            category,
            status: String::new(),
            languages: Vec::new(),
            period: None,
            attestations: Vec::new(),
            legal: Vec::new(),
            events: Vec::new(),
            specialties: Vec::new(),
            source: None,
        }
    }

    /// Reads the metadata an openEHR composition carries, in its canonical
    /// JSON form, for a document in the category `category`.
    ///
    /// openEHR RM 1.1.0 (`COMPOSITION`, `EVENT_CONTEXT`): the version is the
    /// version tree id of `uid`, an `OBJECT_VERSION_ID`
    /// (`object_id::creating_system_id::version_tree_id`), the language is
    /// `language.code_string`, and the period runs from `context.start_time`
    /// to `context.end_time`. Attestations live on the `VERSION`, not in the
    /// composition, so the caller adds them.
    ///
    /// # Errors
    ///
    /// [`DocumentError::MissingMetadata`] when the composition carries no
    /// `uid` naming a version, which Annex III requires.
    pub fn from_composition(
        composition: &serde_json::Value,
        category: CodeableConcept,
    ) -> Result<Self, DocumentError> {
        let text = |pointer: &str| {
            composition
                .pointer(pointer)
                .and_then(serde_json::Value::as_str)
        };
        let version = text("/uid/value")
            .and_then(|uid| uid.split("::").nth(2))
            .filter(|version| !version.is_empty())
            .ok_or(DocumentError::MissingMetadata { element: "version" })?;
        let mut metadata = Self::new(version.to_owned(), category);
        if let Some(language) = text("/language/code_string") {
            metadata.languages.push(language.to_owned());
        }
        let start = text("/context/start_time/value");
        let end = text("/context/end_time/value");
        if start.is_some() || end.is_some() {
            metadata.period = Some(Period {
                start: start.map(Into::into),
                end: end.map(Into::into),
                ..Period::default()
            });
        }
        Ok(metadata)
    }

    /// Sets the status, a code of FHIR R4 `composition-status`.
    #[must_use]
    pub fn with_status(mut self, status: String) -> Self {
        self.status = status;
        self
    }

    /// Adds a language the document is written in, a BCP 47 tag.
    #[must_use]
    pub fn with_language(mut self, language: String) -> Self {
        self.languages.push(language);
        self
    }

    /// Sets the time of service the document records.
    #[must_use]
    pub fn with_period(mut self, period: Period) -> Self {
        self.period = Some(period);
        self
    }

    /// Adds an attestation.
    #[must_use]
    pub fn with_attestation(mut self, attestation: Attestation) -> Self {
        self.attestations.push(attestation);
        self
    }

    /// Adds a legal authentication.
    #[must_use]
    pub fn with_legal_authentication(mut self, authentication: Attestation) -> Self {
        self.legal.push(authentication);
        self
    }

    /// Adds a category of the event the document covers.
    #[must_use]
    pub fn with_event_category(mut self, category: CodeableConcept) -> Self {
        self.events.push(category);
        self
    }

    /// Adds a service specialty.
    #[must_use]
    pub fn with_service_specialty(mut self, specialty: CodeableConcept) -> Self {
        self.specialties.push(specialty);
        self
    }

    /// Sets the URI of the system the document's content comes from, written
    /// as `meta.source` (Xt-EHR D5.2 Table 3 `sourceSystem`).
    #[must_use]
    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    /// Returns the version of the document.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the document category.
    #[must_use]
    pub const fn category(&self) -> &CodeableConcept {
        &self.category
    }

    /// Writes the `DocumentReference` that describes `bundle`, a document
    /// this metadata was written on, with its `id` set to `id`.
    ///
    /// # Errors
    ///
    /// [`DocumentError::Dangling`] when `bundle` holds no `Composition`
    /// first, [`DocumentError::MissingMetadata`] when it carries no
    /// identifier, type, date or title, and [`DocumentError::TooMany`] for
    /// more than one service specialty or language.
    pub fn describe(&self, bundle: &Bundle, id: &str) -> Result<DocumentReference, DocumentError> {
        let header = Header::of_document(bundle)?;
        let title = header.title.clone();
        self.describe_at(
            &header,
            id,
            Attachment {
                content_type: Some(FHIR_JSON.into()),
                title: Some(title.into()),
                ..Attachment::default()
            },
        )
    }

    /// Writes the `DocumentReference` of draft `EEHRxF` Art 3(1)(a): the
    /// PDF/A-1 document `pdf`, its bytes unchanged in the attachment with
    /// their size and SHA-1, described by `header` and this metadata.
    ///
    /// # Errors
    ///
    /// [`DocumentError::TooMany`] for more than one service specialty,
    /// language or legal authentication, and [`DocumentError::TooLarge`]
    /// for a PDF whose size an R4 `unsignedInt` cannot hold.
    pub fn describe_pdf(
        &self,
        header: &Header,
        pdf: &PdfA,
        id: &str,
    ) -> Result<DocumentReference, DocumentError> {
        let size = u32::try_from(pdf.bytes().len()).map_err(|source| DocumentError::TooLarge {
            size: pdf.bytes().len(),
            source,
        })?;
        self.describe_at(
            header,
            id,
            Attachment {
                content_type: Some(PDF.into()),
                data: Some(STANDARD.encode(pdf.bytes()).into()),
                size: Some(size.into()),
                hash: Some(STANDARD.encode(Sha1::digest(pdf.bytes())).into()),
                title: Some(header.title.as_str().into()),
                ..Attachment::default()
            },
        )
    }

    /// Writes the `DocumentReference` that describes the document `header`
    /// names with this metadata, with its `id` set to `id` and its one
    /// content `attachment` as the caller gives it: the URL the document is
    /// read at, its media type and its title; the attachment's language is
    /// this metadata's.
    ///
    /// It writes what [`DocumentMetadata::describe`] writes of a document,
    /// with no written `Bundle`, so a caller that lists documents describes
    /// each before it writes it.
    ///
    /// # Errors
    ///
    /// [`DocumentError::TooMany`] for more than one service specialty,
    /// language or legal authentication.
    pub fn describe_at(
        &self,
        header: &Header,
        id: &str,
        mut attachment: Attachment,
    ) -> Result<DocumentReference, DocumentError> {
        let practice_setting = at_most_one("serviceSpecialty", &self.specialties)?.cloned();
        let language = at_most_one("language", &self.languages)?;
        let legal = at_most_one("legalAuthentication", &self.legal)?;
        attachment.language = language.map(|language| language.as_str().into());
        Ok(DocumentReference {
            id: Some(id.to_owned()),
            meta: self.source.as_ref().map(|source| Meta {
                source: Some(source.as_str().into()),
                ..Meta::default()
            }),
            extension: vec![version_extension(DOCUMENT_REFERENCE_VERSION, &self.version)],
            master_identifier: Some(header.identifier.clone()),
            identifier: header.composition.iter().cloned().collect(),
            status: "current".into(),
            doc_status: Some(header.status.as_str().into()),
            r#type: Some(header.kind.clone()),
            category: vec![self.category.clone()],
            subject: Some(header.subject.clone()),
            date: Some(header.date.as_str().into()),
            author: header.authors.clone(),
            authenticator: legal.map(|legal| reference(&legal.party)),
            custodian: header.custodian.clone(),
            description: Some(header.title.as_str().into()),
            content: vec![DocumentReferenceContent {
                attachment,
                ..DocumentReferenceContent::default()
            }],
            // NOTE: R4 Element ele-1: an element holds a value or children, so a context with
            // neither event, period nor practice setting is left out.
            context: Some(DocumentReferenceContext {
                event: self.events.clone(),
                period: self.period.clone(),
                practice_setting,
                ..DocumentReferenceContext::default()
            })
            .filter(|context| *context != DocumentReferenceContext::default()),
            ..DocumentReference::default()
        })
    }
}

/// The Annex III Table 1 elements a [`Document`] writes on its parts: what a
/// `DocumentReference` describes beside the [`DocumentMetadata`].
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    identifier: Identifier,
    composition: Option<Identifier>,
    subject: Reference,
    authors: Vec<Reference>,
    date: String,
    kind: CodeableConcept,
    title: String,
    custodian: Option<Reference>,
    status: String,
}

impl Header {
    /// Creates the header of the document `identifier` about `subject`,
    /// issued at `date`, a FHIR `instant`, of the type `kind` (Table 1
    /// `documentType`), titled `title`, by `author`, with the status
    /// `final`.
    #[must_use]
    pub fn new(
        identifier: Identifier,
        subject: Reference,
        date: String,
        kind: CodeableConcept,
        title: String,
        author: Reference,
    ) -> Self {
        Self {
            identifier,
            composition: None,
            subject,
            authors: vec![author],
            date,
            kind,
            title,
            custodian: None,
            status: String::from("final"),
        }
    }

    /// Adds an author.
    #[must_use]
    pub fn with_author(mut self, author: Reference) -> Self {
        self.authors.push(author);
        self
    }

    /// Sets the custodian.
    #[must_use]
    pub fn with_custodian(mut self, custodian: Reference) -> Self {
        self.custodian = Some(custodian);
        self
    }

    /// Reads the header of `bundle`, a document [`Document::into_bundle`]
    /// wrote: the `Bundle` identifier and timestamp and the parts of its
    /// `Composition`.
    ///
    /// # Errors
    ///
    /// [`DocumentError::Dangling`] when `bundle` holds no `Composition`
    /// first, and [`DocumentError::MissingMetadata`] when it carries no
    /// identifier, subject, author, type, date or title.
    pub fn of_document(bundle: &Bundle) -> Result<Self, DocumentError> {
        let Some(Resource::Composition(composition)) = bundle
            .entry
            .first()
            .and_then(|entry| entry.resource.as_ref())
        else {
            return Err(DocumentError::Dangling {
                slice: "composition",
            });
        };
        let missing = |element| DocumentError::MissingMetadata { element };
        if composition.author.is_empty() {
            return Err(missing("author"));
        }
        if composition.r#type.coding.is_empty() {
            return Err(missing("documentType"));
        }
        Ok(Self {
            identifier: bundle
                .identifier
                .clone()
                .ok_or_else(|| missing("identifier"))?,
            composition: composition.identifier.clone(),
            subject: composition
                .subject
                .clone()
                .ok_or_else(|| missing("subject"))?,
            authors: composition.author.clone(),
            date: bundle
                .timestamp
                .as_ref()
                .and_then(|timestamp| timestamp.value.clone())
                .ok_or_else(|| missing("date"))?,
            kind: composition.r#type.clone(),
            title: composition
                .title
                .value
                .clone()
                .filter(|title| !title.is_empty())
                .ok_or_else(|| missing("documentTitle"))?,
            custodian: composition.custodian.clone(),
            status: composition
                .status
                .value
                .clone()
                .unwrap_or_else(|| String::from("final")),
        })
    }
}

/// The one value of `values`, `None` for none, refusing a second one that
/// R4 has no place for.
fn at_most_one<'a, T>(
    element: &'static str,
    values: &'a [T],
) -> Result<Option<&'a T>, DocumentError> {
    match values {
        [] => Ok(None),
        [one] => Ok(Some(one)),
        more => Err(DocumentError::TooMany {
            element,
            given: more.len(),
        }),
    }
}

/// The cross-version extension `url` carrying `version`.
fn version_extension(url: &str, version: &str) -> Extension {
    Extension {
        url: url.into(),
        value: Some(ExtensionValue::String(version.into())),
        ..Extension::default()
    }
}

/// Fills the Annex III Table 2 metadata of one data group from the document:
/// its `date` from `date` and its `language` from `language` where the
/// resource carries none, and refuses a data group with no subject.
///
/// openEHR RM 1.1.0: an `ENTRY` is finalised with the `COMPOSITION` that
/// holds it, so the document's date is the date of each of its data groups.
fn data_group(
    resource: &mut Resource,
    date: &str,
    language: Option<&str>,
) -> Result<(), DocumentError> {
    let (kind, subject, recorded, written): (
        &'static str,
        Option<&Reference>,
        Option<&mut Option<DateTime>>,
        &mut Option<Code>,
    ) = match resource {
        Resource::Condition(r) => (
            "Condition",
            Some(&r.subject),
            Some(&mut r.recorded_date),
            &mut r.language,
        ),
        Resource::AllergyIntolerance(r) => (
            "AllergyIntolerance",
            Some(&r.patient),
            Some(&mut r.recorded_date),
            &mut r.language,
        ),
        Resource::MedicationStatement(r) => (
            "MedicationStatement",
            Some(&r.subject),
            Some(&mut r.date_asserted),
            &mut r.language,
        ),
        Resource::MedicationRequest(r) => (
            "MedicationRequest",
            Some(&r.subject),
            Some(&mut r.authored_on),
            &mut r.language,
        ),
        Resource::Immunization(r) => (
            "Immunization",
            Some(&r.patient),
            Some(&mut r.recorded),
            &mut r.language,
        ),
        Resource::DeviceUseStatement(r) => (
            "DeviceUseStatement",
            Some(&r.subject),
            Some(&mut r.recorded_on),
            &mut r.language,
        ),
        Resource::CarePlan(r) => (
            "CarePlan",
            Some(&r.subject),
            Some(&mut r.created),
            &mut r.language,
        ),
        Resource::ServiceRequest(r) => (
            "ServiceRequest",
            Some(&r.subject),
            Some(&mut r.authored_on),
            &mut r.language,
        ),
        Resource::Procedure(r) => ("Procedure", Some(&r.subject), None, &mut r.language),
        Resource::Observation(r) => ("Observation", r.subject.as_ref(), None, &mut r.language),
        Resource::MedicationDispense(r) => (
            "MedicationDispense",
            r.subject.as_ref(),
            None,
            &mut r.language,
        ),
        _ => return Ok(()),
    };
    if subject.is_none_or(|subject| subject.reference.is_none() && subject.identifier.is_none()) {
        return Err(DocumentError::DataGroup {
            resource: kind,
            element: "subject",
        });
    }
    if let Some(recorded) = recorded {
        recorded.get_or_insert_with(|| date.into());
    }
    if let Some(language) = language {
        written.get_or_insert_with(|| language.into());
    }
    Ok(())
}

impl Document {
    /// Sets the Annex III Table 1 metadata the document writes on its
    /// `Composition`.
    pub fn with_metadata(&mut self, metadata: DocumentMetadata) {
        self.metadata = Some(metadata);
    }

    /// Whether the metadata set names a legal authentication.
    pub(super) fn legally_authenticated(&self) -> bool {
        self.metadata
            .as_ref()
            .is_some_and(|metadata| !metadata.legal.is_empty())
    }

    /// Fills the Annex III Table 2 metadata of every data group the document
    /// holds, once metadata is set.
    pub(super) fn fill_data_groups(&mut self) -> Result<(), DocumentError> {
        let Some(metadata) = &self.metadata else {
            return Ok(());
        };
        let language = at_most_one("language", &metadata.languages)?.map(String::as_str);
        for (_, resource) in &mut self.entries {
            data_group(resource, &self.timestamp, language)?;
        }
        Ok(())
    }

    /// Writes the Annex III document category on `report`, which the
    /// composition then carries with the report's own categories.
    // NOTE: Bundle-eu-lab dr-comp-category asks the two to share a category, and
    // DiagnosticReport-eu-lab 2.0.0 slices category open.
    pub(super) fn categorise_report(&self, report: &mut DiagnosticReport) {
        if let Some(metadata) = &self.metadata
            && self.form.categories.is_empty()
            && !report.category.contains(&metadata.category)
        {
            report.category.push(metadata.category.clone());
        }
    }

    /// Writes the metadata on `composition`, the document's own, refusing a
    /// value R4 has no place for.
    pub(super) fn write_metadata(
        &self,
        composition: &mut Composition,
    ) -> Result<(), DocumentError> {
        let Some(metadata) = &self.metadata else {
            return Ok(());
        };
        if composition.author.is_empty() {
            return Err(DocumentError::MissingMetadata { element: "author" });
        }
        composition
            .extension
            .push(version_extension(COMPOSITION_VERSION, &metadata.version));
        if !metadata.status.is_empty() {
            composition.status = metadata.status.as_str().into();
        }
        if let Some(language) = at_most_one("language", &metadata.languages)? {
            composition.language = Some(language.as_str().into());
        }
        // NOTE: Composition-eu-lab 2.0.0 slices category open; the imaging form fixes it (#2120).
        if self.form.categories.is_empty() && !composition.category.contains(&metadata.category) {
            composition.category.push(metadata.category.clone());
        }
        let attester = |mode: &str, attestation: &Attestation| CompositionAttester {
            mode: mode.into(),
            time: Some(attestation.time.as_str().into()),
            party: Some(reference(&attestation.party)),
            ..CompositionAttester::default()
        };
        composition.attester.extend(
            metadata
                .attestations
                .iter()
                .map(|a| attester("professional", a)),
        );
        composition
            .attester
            .extend(metadata.legal.iter().map(|a| attester("legal", a)));
        if !metadata.events.is_empty() || metadata.period.is_some() {
            composition.event.push(CompositionEvent {
                code: metadata.events.clone(),
                period: metadata.period.clone(),
                ..CompositionEvent::default()
            });
        }
        if let Some(source) = &metadata.source {
            composition.meta.get_or_insert_with(Meta::default).source =
                Some(source.as_str().into());
        }
        Ok(())
    }
}
