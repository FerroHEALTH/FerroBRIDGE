// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The technical representations of draft `EEHRxF` Art 3(1) and Annex II
//! (initiative 17512, 2026-11-23): the PDF/A-1 document embedded in a
//! `DocumentReference`, the narrative-only document, and what a system
//! declares per category.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use eehrxf::category::Category;
use eehrxf::document::metadata::{DocumentMetadata, Header, PDF};
use eehrxf::document::representation::{
    Conformance, Declarations, PdfA, Representation, RepresentationError,
};
use eehrxf::document::{DocumentError, LOINC};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::document_reference::DocumentReference;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::reference::Reference;
use sha1::{Digest as _, Sha1};

/// A synthetic PDF whose XMP metadata declares `part` and `level`, with a
/// byte above 0x7F so a round trip is held to every byte.
fn pdf(part: &str, level: &str) -> Vec<u8> {
    let mut bytes =
        b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n1 0 obj\n<< /Type /Metadata /Subtype /XML >>\nstream\n"
            .to_vec();
    bytes.extend_from_slice(
        format!(
            "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:Description \
             xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\" pdfaid:part=\"{part}\" \
             pdfaid:conformance=\"{level}\"/></x:xmpmeta>"
        )
        .as_bytes(),
    );
    bytes.extend_from_slice(b"\nendstream\nendobj\n%%EOF\n");
    bytes
}

/// The LOINC concept `code`.
fn loinc(code: &str) -> CodeableConcept {
    CodeableConcept {
        coding: vec![Coding {
            system: Some(LOINC.into()),
            code: Some(code.into()),
            ..Coding::default()
        }],
        ..CodeableConcept::default()
    }
}

/// A reference to `target`.
fn to(target: &str) -> Reference {
    Reference {
        reference: Some(target.into()),
        ..Reference::default()
    }
}

/// The `DocumentReference` of a synthetic laboratory report kept as `pdf`.
fn described(pdf: &PdfA) -> Result<DocumentReference, DocumentError> {
    let header = Header::new(
        Identifier {
            system: Some("urn:ietf:rfc:3986".into()),
            value: Some("urn:uuid:5b0c3f7e-0000-4000-8000-00000000f001".into()),
            ..Identifier::default()
        },
        to("Patient/patient-1"),
        String::from("2026-10-25T12:00:00Z"),
        loinc("11502-2"),
        String::from("Laboratory report"),
        to("Organization/laboratory"),
    );
    DocumentMetadata::new(String::from("1"), loinc("11502-2"))
        .with_language(String::from("en"))
        .describe_pdf(&header, pdf, "pdf-report")
}

#[test]
fn a_pdf_a_1b_document_is_accepted_with_its_level() -> Result<(), Box<dyn Error>> {
    assert_eq!(PdfA::new(pdf("1", "B"))?.conformance(), Conformance::B);
    assert_eq!(PdfA::new(pdf("1", "A"))?.conformance(), Conformance::A);
    Ok(())
}

#[test]
fn a_pdf_a_2b_document_is_refused() {
    assert!(matches!(
        PdfA::new(pdf("2", "B")),
        Err(RepresentationError::NotPdfA1 { .. })
    ));
}

#[test]
fn a_pdf_without_an_identification_schema_is_refused() {
    assert!(matches!(
        PdfA::new(b"%PDF-1.4\n%%EOF\n".to_vec()),
        Err(RepresentationError::NotPdfA1 { .. })
    ));
}

#[test]
fn bytes_that_are_no_pdf_are_refused() {
    assert!(matches!(
        PdfA::new(b"<html></html>".to_vec()),
        Err(RepresentationError::NotPdf)
    ));
}

#[test]
fn the_pdf_travels_byte_for_byte_with_its_size_and_hash() -> Result<(), Box<dyn Error>> {
    let bytes = pdf("1", "B");
    let reference = described(&PdfA::new(bytes.clone())?)?;
    // Through the wire form and back.
    let wire = serde_json::to_string(&reference)?;
    let read: serde_json::Value = serde_json::from_str(&wire)?;
    let attachment = &read["content"][0]["attachment"];
    assert_eq!(attachment["contentType"], PDF);
    let data = attachment["data"].as_str().ok_or("the data")?;
    assert_eq!(STANDARD.decode(data)?, bytes);
    assert_eq!(attachment["size"], bytes.len());
    assert_eq!(attachment["hash"], STANDARD.encode(Sha1::digest(&bytes)));
    assert_eq!(attachment["language"], "en");
    // DocumentReference-minimal on the PDF form too.
    assert!(read["masterIdentifier"]["value"].is_string());
    assert_eq!(read["type"]["coding"][0]["code"], "11502-2");
    assert_eq!(read["subject"]["reference"], "Patient/patient-1");
    assert_eq!(read["date"], "2026-10-25T12:00:00Z");
    assert_eq!(read["description"], "Laboratory report");
    Ok(())
}

// NOTE: draft EEHRxF Art 3(1)(a): the PDF form of a laboratory report, measured by the draft
// corpus on its embedded PDF and its Annex III metadata.
#[test]
fn a_pdf_laboratory_report_is_written_for_the_corpus() -> Result<(), Box<dyn Error>> {
    let reference = described(&PdfA::new(pdf("1", "B"))?)?;
    assert!(
        reference.context.is_none(),
        "R4 ele-1: a context with nothing to hold is left out"
    );
    crate::support::emit("eehrxf-pdf-laboratory", &serde_json::to_string(&reference)?)
}

#[test]
#[cfg(feature = "discharge")]
fn a_narrative_only_discharge_report_is_written() -> Result<(), Box<dyn Error>> {
    let mut document = super::discharge_document::document(true, true)?;
    document.narrative_only();
    let bundle = serde_json::to_value(document.into_bundle()?)?;
    let sections = bundle["entry"][0]["resource"]["section"]
        .as_array()
        .ok_or("sections")?;
    assert_ne!(sections.len(), 0);
    for section in sections {
        assert!(section["text"]["div"].is_string());
        assert!(section.get("entry").is_none());
    }
    Ok(())
}

#[test]
#[cfg(feature = "laboratory")]
fn a_narrative_only_document_with_an_entry_is_refused() -> Result<(), Box<dyn Error>> {
    let mut document = super::lab_document::document()?;
    document.narrative_only();
    assert!(matches!(
        document.into_bundle(),
        Err(DocumentError::NarrativeOnly { .. })
    ));
    Ok(())
}

#[test]
fn declarations_are_read_as_configuration_and_held_to_annex_ii() -> Result<(), Box<dyn Error>> {
    let mut map = serde_json::Map::new();
    for category in Category::ENABLED {
        map.insert(
            category.code().to_owned(),
            serde_json::json!(["pdf", "structured"]),
        );
    }
    let declared: Declarations = serde_json::from_value(serde_json::Value::Object(map))?;
    declared.check()?;
    for category in Category::ENABLED {
        assert!(declared.declares(*category, Representation::Pdf));
        assert!(!declared.declares(*category, Representation::Narrative));
    }
    Declarations::structured().check()?;
    Ok(())
}

#[test]
fn a_declaration_annex_ii_does_not_admit_is_refused() -> Result<(), Box<dyn Error>> {
    let Some(category) = Category::ENABLED
        .iter()
        .find(|category| category.code() != "Medical-Imaging")
    else {
        return Ok(());
    };
    let declared: Declarations =
        serde_json::from_value(serde_json::json!({ category.code(): ["dicom"] }))?;
    assert!(matches!(
        declared.check(),
        Err(RepresentationError::NotAdmitted { .. })
    ));
    let unknown: Declarations = serde_json::from_value(serde_json::json!({"Recipes": ["pdf"]}))?;
    assert!(matches!(
        unknown.check(),
        Err(RepresentationError::Category(_))
    ));
    Ok(())
}
