// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The draft `EEHRxF` Annex III metadata (initiative 17512, 2026-11-23) on
//! each category's document and on the `DocumentReference` that describes
//! it, the shape MyHealth@EU DocumentReference-minimal constrains.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::document::metadata::{
    Attestation, COMPOSITION_VERSION, DOCUMENT_REFERENCE_VERSION, DocumentMetadata, FHIR_JSON,
};
use eehrxf::document::{Document, DocumentError, LOINC};
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::observation::Observation;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::resource::Resource;
use serde_json::{Value, json};

/// The LOINC document category `code`.
fn category(code: &str) -> CodeableConcept {
    CodeableConcept {
        coding: vec![Coding {
            system: Some(LOINC.into()),
            code: Some(code.into()),
            ..Coding::default()
        }],
        ..CodeableConcept::default()
    }
}

/// The metadata of a synthetic composition in the category `code`.
fn metadata(code: &str) -> Result<DocumentMetadata, DocumentError> {
    let composition = json!({
        "uid": {"value": "8849182c-82ad-4088-a07f-48ead4180515::synthetic.example.org::2"},
        "language": {"code_string": "en"},
        "context": {
            "start_time": {"value": "2026-10-25T09:00:00Z"},
            "end_time": {"value": "2026-10-25T11:00:00Z"}
        }
    });
    Ok(
        DocumentMetadata::from_composition(&composition, category(code))?
            .with_event_category(category("26436-6"))
            .with_service_specialty(category("394915009"))
            .with_attestation(Attestation::new(
                String::from("Patient/synthetic-subject"),
                String::from("2026-10-25T11:30:00Z"),
            ))
            .with_source(String::from("urn:oid:2.999.1")),
    )
}

/// Holds `bundle`, written with [`metadata`], and the `DocumentReference`
/// describing it to every mandatory element of Annex III Table 1.
fn holds_table_1(metadata: &DocumentMetadata, bundle: &Bundle) -> Result<(), Box<dyn Error>> {
    let reference = serde_json::to_value(metadata.describe(bundle, "description")?)?;
    let bundle = serde_json::to_value(bundle)?;
    let composition = &bundle["entry"][0]["resource"];
    // Table 1: subject, identifier, version, author, date, status, documentType,
    // documentTitle on the composition.
    assert!(composition["subject"]["reference"].is_string());
    assert!(composition["identifier"]["value"].is_string());
    assert_eq!(extension(composition, COMPOSITION_VERSION), Some("2"));
    assert!(composition["author"][0].is_object());
    assert!(composition["date"].is_string());
    assert!(composition["status"].is_string());
    assert!(composition["type"]["coding"][0]["code"].is_string());
    assert!(composition["title"].is_string());
    assert_eq!(composition["language"], "en");
    assert_eq!(composition["attester"][0]["mode"], "professional");
    assert_eq!(
        composition["event"][0]["period"]["start"],
        "2026-10-25T09:00:00Z"
    );
    assert_eq!(composition["meta"]["source"], "urn:oid:2.999.1");
    // DocumentReference-minimal: masterIdentifier, type, subject, date and
    // description, 1..1 each.
    assert_eq!(reference["resourceType"], "DocumentReference");
    assert_eq!(reference["masterIdentifier"], bundle["identifier"]);
    assert_eq!(reference["type"], composition["type"]);
    assert_eq!(reference["subject"], composition["subject"]);
    assert_eq!(reference["date"], bundle["timestamp"]);
    assert_eq!(reference["description"], composition["title"]);
    assert_eq!(reference["status"], "current");
    assert_eq!(reference["docStatus"], composition["status"]);
    assert_eq!(
        reference["category"][0],
        serde_json::to_value(metadata.category())?
    );
    assert_eq!(extension(&reference, DOCUMENT_REFERENCE_VERSION), Some("2"));
    assert_eq!(
        reference["content"][0]["attachment"]["contentType"],
        FHIR_JSON
    );
    assert_eq!(reference["content"][0]["attachment"]["language"], "en");
    assert_eq!(
        reference["context"]["event"][0]["coding"][0]["code"],
        "26436-6"
    );
    assert_eq!(
        reference["context"]["practiceSetting"]["coding"][0]["code"],
        "394915009"
    );
    Ok(())
}

/// The `valueString` of the extension `url` of `resource`.
fn extension<'a>(resource: &'a Value, url: &str) -> Option<&'a str> {
    resource["extension"]
        .as_array()?
        .iter()
        .find(|extension| extension["url"] == url)?["valueString"]
        .as_str()
}

/// Adds a synthetic organization as an author of `document`.
fn authored(mut document: Document) -> Result<Document, DocumentError> {
    document.with_author(&Resource::Organization(Box::new(Organization {
        id: Some(String::from("hospital")),
        name: Some("Synthetic hospital".into()),
        ..Organization::default()
    })))?;
    Ok(document)
}

/// The `Document`, authored, written with the metadata, and the metadata.
fn written(document: Document, code: &str) -> Result<(Bundle, DocumentMetadata), Box<dyn Error>> {
    let mut document = authored(document)?;
    let metadata = metadata(code)?;
    document.with_metadata(metadata.clone());
    Ok((document.into_bundle()?, metadata))
}

#[test]
#[cfg(feature = "laboratory")]
fn the_laboratory_report_carries_the_annex_iii_document_metadata() -> Result<(), Box<dyn Error>> {
    let (bundle, metadata) = written(super::lab_document::document()?, "11502-2")?;
    holds_table_1(&metadata, &bundle)
}

#[test]
#[cfg(feature = "laboratory")]
fn every_laboratory_data_group_carries_its_annex_iii_date_and_language()
-> Result<(), Box<dyn Error>> {
    let (bundle, _) = written(super::lab_document::document()?, "11502-2")?;
    let bundle = serde_json::to_value(bundle)?;
    for entry in bundle["entry"].as_array().ok_or("entries")? {
        let resource = &entry["resource"];
        if resource["resourceType"] == "Observation" {
            assert_eq!(resource["language"], "en");
        }
    }
    Ok(())
}

#[test]
#[cfg(feature = "imaging")]
fn the_imaging_report_carries_the_annex_iii_document_metadata() -> Result<(), Box<dyn Error>> {
    let (bundle, metadata) = written(super::imaging_document::document(None)?, "18748-4")?;
    holds_table_1(&metadata, &bundle)
}

#[test]
#[cfg(feature = "discharge")]
fn the_discharge_report_carries_the_annex_iii_document_metadata() -> Result<(), Box<dyn Error>> {
    let (bundle, metadata) = written(super::discharge_document::document(true, true)?, "34105-7")?;
    holds_table_1(&metadata, &bundle)?;
    // A form that leaves Composition.category free carries the category.
    let bundle = serde_json::to_value(bundle)?;
    assert!(
        bundle["entry"][0]["resource"]["category"]
            .as_array()
            .ok_or("categories")?
            .iter()
            .any(|category| category["coding"][0]["code"] == "34105-7")
    );
    Ok(())
}

#[test]
fn a_composition_without_a_version_is_refused() {
    let composition = json!({"language": {"code_string": "en"}});
    assert!(matches!(
        DocumentMetadata::from_composition(&composition, category("11502-2")),
        Err(DocumentError::MissingMetadata { element: "version" })
    ));
}

#[test]
#[cfg(feature = "laboratory")]
fn a_second_language_is_refused_where_r4_holds_one() -> Result<(), Box<dyn Error>> {
    let mut document = authored(super::lab_document::document()?)?;
    document.with_metadata(metadata("11502-2")?.with_language(String::from("nl")));
    assert!(matches!(
        document.into_bundle(),
        Err(DocumentError::TooMany {
            element: "language",
            given: 2
        })
    ));
    Ok(())
}

#[test]
#[cfg(feature = "laboratory")]
fn a_second_service_specialty_is_refused_where_r4_holds_one() -> Result<(), Box<dyn Error>> {
    let metadata = metadata("11502-2")?.with_service_specialty(category("408478003"));
    let mut document = authored(super::lab_document::document()?)?;
    document.with_metadata(metadata.clone());
    let bundle = document.into_bundle()?;
    assert!(matches!(
        metadata.describe(&bundle, "description"),
        Err(DocumentError::TooMany {
            element: "serviceSpecialty",
            given: 2
        })
    ));
    Ok(())
}

#[test]
#[cfg(feature = "discharge")]
fn a_document_without_an_author_is_refused() -> Result<(), Box<dyn Error>> {
    let mut document = super::discharge_document::document(true, true)?;
    document.with_metadata(metadata("34105-7")?);
    assert!(matches!(
        document.into_bundle(),
        Err(DocumentError::MissingMetadata { element: "author" })
    ));
    Ok(())
}

#[test]
#[cfg(feature = "laboratory")]
fn a_data_group_without_a_subject_is_refused() -> Result<(), Box<dyn Error>> {
    let mut document = authored(super::lab_document::document()?)?;
    document.add(&[Resource::Observation(Box::new(Observation {
        id: Some(String::from("no-subject")),
        status: "final".into(),
        code: category("2345-7"),
        ..Observation::default()
    }))])?;
    document.with_metadata(metadata("11502-2")?);
    assert!(matches!(
        document.into_bundle(),
        Err(DocumentError::DataGroup {
            resource: "Observation",
            element: "subject"
        })
    ));
    Ok(())
}
