// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The discharge report in the MyHealth@EU Hospital Discharge Report 9.2.0
//! form: its language-region tag from the record (openEHR RM 1.1.0
//! `COMPOSITION.language` and `territory`, RFC 5646 §2.1), held to the
//! eHDSILanguage value set `composition-hdr-myhealtheu` binds through
//! `ValueSet/$validate-code`, and its author the one the record names.
#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use eehrxf::document::authored::{
    MYHEALTHEU_ORGANIZATION, MYHEALTHEU_PRACTITIONER, recorded_author,
};
use eehrxf::document::discharge::{
    DISCHARGE_REPORT_FORM, HDR_SLOTS, MYHEALTHEU_DISCHARGE_REPORT_FORM, MYHEALTHEU_HDR_BUNDLE,
    MYHEALTHEU_HDR_COMPOSITION, MYHEALTHEU_HDR_ENCOUNTER,
};
use eehrxf::document::language::{DocumentLanguage, EHDSI_LANGUAGE, LanguageError};
use eehrxf::document::section::Section;
use eehrxf::document::{Document, DocumentError, MYHEALTHEU_PATIENT};
use eehrxf::terminology::{Terminology, TerminologyError};
use fhir_types::r4::coding::Coding;
use fhir_types::r4::device::Device;
use fhir_types::r4::encounter::Encounter;
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::resource::Resource;
use http::StatusCode;
use serde_json::json;

use crate::terminology::{Script, Scripted};

/// The terminology server that answers every `$validate-code` with `valid`.
fn answering(valid: bool) -> Terminology {
    Terminology::server(Arc::new(Scripted(Script::Valid(valid))))
}

/// A synthetic composition composed by `composer`, at `facility`.
fn composition(composer: &serde_json::Value, facility: &serde_json::Value) -> serde_json::Value {
    json!({
        "language": {"code_string": "en"},
        "territory": {"code_string": "GB"},
        "composer": composer,
        "context": {"health_care_facility": facility}
    })
}

/// A MyHealth@EU discharge report with its patient, its stay and a hospital
/// course, before its author and language.
fn bare() -> Result<Document, Box<dyn Error>> {
    let mut document = Document::of(
        &MYHEALTHEU_DISCHARGE_REPORT_FORM,
        "https://bridge.example.org/fhir",
        String::from("urn:uuid:5b0c3f7e-0000-4000-8000-00000000e001"),
        String::from("2026-10-25T12:00:00Z"),
        String::from("Discharge report"),
    );
    document.with_patient(Patient {
        id: Some(String::from("patient-1")),
        name: vec![HumanName {
            family: Some("Synthetic".into()),
            ..HumanName::default()
        }],
        ..Patient::default()
    })?;
    document.with_encounter(Encounter {
        id: Some(String::from("stay-1")),
        status: "finished".into(),
        class: Coding {
            system: Some("http://terminology.hl7.org/CodeSystem/v3-ActCode".into()),
            code: Some("IMP".into()),
            ..Coding::default()
        },
        ..Encounter::default()
    })?;
    let course = HDR_SLOTS
        .iter()
        .find(|slot| slot.slice == "sectionHospitalCourse")
        .ok_or("the hospital course slot")?;
    document.with_section(Section::new(
        course,
        vec![String::from("A synthetic stay.")],
    ))?;
    Ok(document)
}

#[test]
fn the_language_joins_the_record_s_language_and_territory() -> Result<(), Box<dyn Error>> {
    assert_eq!(DocumentLanguage::new("en", "GB")?.tag(), "en-GB");
    assert_eq!(DocumentLanguage::new("NL", "nl")?.tag(), "nl-NL");
    let read = DocumentLanguage::of_composition(&composition(&json!(null), &json!(null)))?;
    assert_eq!(read.tag(), "en-GB");
    Ok(())
}

#[test]
fn a_bare_or_malformed_code_is_refused() {
    assert!(matches!(
        DocumentLanguage::new("eng", "GB"),
        Err(LanguageError::Language { .. })
    ));
    assert!(matches!(
        DocumentLanguage::new("en", ""),
        Err(LanguageError::Territory { .. })
    ));
    assert!(matches!(
        DocumentLanguage::of_composition(&json!({"language": {"code_string": "en"}})),
        Err(LanguageError::Unrecorded {
            attribute: "territory"
        })
    ));
}

#[tokio::test]
async fn the_bound_value_set_is_asked_and_its_answer_decides() -> Result<(), Box<dyn Error>> {
    let held = DocumentLanguage::new("en", "GB")?
        .held(&answering(true), EHDSI_LANGUAGE)
        .await?;
    assert_eq!(held.language().tag(), "en-GB");
    assert_eq!(held.value_set(), EHDSI_LANGUAGE);
    assert!(matches!(
        DocumentLanguage::new("en", "AQ")?
            .held(&answering(false), EHDSI_LANGUAGE)
            .await,
        Err(LanguageError::Unheld { tag, .. }) if tag == "en-AQ"
    ));
    Ok(())
}

#[tokio::test]
async fn no_terminology_answer_is_never_a_pass() -> Result<(), Box<dyn Error>> {
    assert!(matches!(
        DocumentLanguage::new("en", "GB")?
            .held(&Terminology::none(), EHDSI_LANGUAGE)
            .await,
        Err(LanguageError::Terminology {
            source: TerminologyError::Unconfigured { .. },
            ..
        })
    ));
    let refusing = Terminology::server(Arc::new(Scripted(Script::Refuse(
        StatusCode::SERVICE_UNAVAILABLE,
    ))));
    assert!(matches!(
        DocumentLanguage::new("en", "GB")?
            .held(&refusing, EHDSI_LANGUAGE)
            .await,
        Err(LanguageError::Terminology {
            source: TerminologyError::Status { .. },
            ..
        })
    ));
    Ok(())
}

#[tokio::test]
async fn a_bound_form_takes_only_a_language_its_value_set_holds() -> Result<(), Box<dyn Error>> {
    let mut document = bare()?;
    assert!(matches!(
        document.with_language(DocumentLanguage::new("en", "GB")?),
        Err(DocumentError::UnheldLanguage {
            value_set: EHDSI_LANGUAGE
        })
    ));
    let elsewhere = DocumentLanguage::new("en", "GB")?
        .held(&answering(true), "http://example.org/fhir/ValueSet/other")
        .await?;
    assert!(matches!(
        document.with_held_language(elsewhere),
        Err(DocumentError::UnheldLanguage { .. })
    ));
    // The HL7 Europe form binds no value set, so it takes the language as built.
    let mut unbound = Document::of(
        &DISCHARGE_REPORT_FORM,
        "https://bridge.example.org/fhir",
        String::from("urn:uuid:5b0c3f7e-0000-4000-8000-00000000e002"),
        String::from("2026-10-25T12:00:00Z"),
        String::from("Discharge report"),
    );
    unbound.with_language(DocumentLanguage::new("en", "GB")?)?;
    Ok(())
}

#[test]
fn the_record_s_author_is_its_identified_composer_or_else_its_facility()
-> Result<(), Box<dyn Error>> {
    let composer = json!({
        "_type": "PARTY_IDENTIFIED",
        "name": "Dr. Synthetic",
        "identifiers": [{"_type": "DV_IDENTIFIER", "id": "SYN-1", "issuer": "example", "assigner": "example", "type": "local"}]
    });
    let facility = json!({"_type": "PARTY_IDENTIFIED", "name": "Synthetic hospital"});
    let Resource::Practitioner(practitioner) = recorded_author(&composition(&composer, &facility))?
    else {
        return Err("the identified composer is a Practitioner".into());
    };
    let practitioner = serde_json::to_value(&practitioner)?;
    assert_eq!(practitioner["name"][0]["text"], "Dr. Synthetic");
    assert_eq!(practitioner["identifier"][0]["value"], "SYN-1");
    assert_eq!(practitioner["meta"]["profile"][0], MYHEALTHEU_PRACTITIONER);
    let Resource::Organization(organization) =
        recorded_author(&composition(&json!({"_type": "PARTY_SELF"}), &facility))?
    else {
        return Err("the facility is an Organization".into());
    };
    let organization = serde_json::to_value(&organization)?;
    assert_eq!(organization["name"], "Synthetic hospital");
    assert_eq!(organization["meta"]["profile"][0], MYHEALTHEU_ORGANIZATION);
    assert!(matches!(
        recorded_author(&composition(&json!({"_type": "PARTY_SELF"}), &json!(null))),
        Err(DocumentError::Unauthored)
    ));
    assert!(matches!(
        recorded_author(&composition(
            &json!({"_type": "PARTY_IDENTIFIED", "name": "Dr. Synthetic", "identifiers": [{"issuer": "example"}]}),
            &json!(null)
        )),
        Err(DocumentError::AuthorIdentifier)
    ));
    Ok(())
}

#[tokio::test]
async fn a_device_author_is_refused_and_the_record_s_author_is_taken() -> Result<(), Box<dyn Error>>
{
    let held = DocumentLanguage::new("en", "GB")?
        .held(&answering(true), EHDSI_LANGUAGE)
        .await?;
    let mut gateway = bare()?;
    gateway.with_held_language(held.clone())?;
    gateway.with_author(&Resource::Device(Box::new(Device {
        id: Some(String::from("gateway")),
        ..Device::default()
    })))?;
    assert!(matches!(
        gateway.into_bundle(),
        Err(DocumentError::Author { full_url }) if full_url.ends_with("/Device/gateway")
    ));
    let mut unauthored = bare()?;
    unauthored.with_held_language(held.clone())?;
    assert!(matches!(
        unauthored.into_bundle(),
        Err(DocumentError::Unauthored)
    ));
    let mut document = bare()?;
    document.with_held_language(held)?;
    document.with_author(&recorded_author(&composition(
        &json!({"_type": "PARTY_IDENTIFIED", "name": "Dr. Synthetic"}),
        &json!(null),
    ))?)?;
    let bundle = serde_json::to_value(document.into_bundle()?)?;
    assert_eq!(bundle["meta"]["profile"][0], MYHEALTHEU_HDR_BUNDLE);
    assert_eq!(bundle["language"], "en-GB");
    let composition = &bundle["entry"][0]["resource"];
    assert_eq!(
        composition["meta"]["profile"][0],
        MYHEALTHEU_HDR_COMPOSITION
    );
    assert_eq!(composition["language"], "en-GB");
    let entries = bundle["entry"].as_array().ok_or("entries")?;
    let profile_of = |kind: &str| {
        entries
            .iter()
            .find(|entry| entry["resource"]["resourceType"] == kind)
            .map(|entry| entry["resource"]["meta"]["profile"][0].clone())
    };
    assert_eq!(profile_of("Patient"), Some(json!(MYHEALTHEU_PATIENT)));
    assert_eq!(
        profile_of("Encounter"),
        Some(json!(MYHEALTHEU_HDR_ENCOUNTER))
    );
    Ok(())
}
