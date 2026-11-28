// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concepts of a mapped resource in no preferred system, translated
//! through a scripted terminology server: an `equivalent` or `equal` match
//! added after the original coding, a concept in a preferred system left
//! alone, no server leaving the code as recorded (eHN PS Art 10(7)), a
//! refusal failing the translation, a concept at an element a target
//! profile binds asked for the binding's system, and each added coding
//! given its designation in a requested language.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use eehrxf::terminology::Operation;
use eehrxf::terminology::Terminology;
use eehrxf::terminology::TerminologyError;
use eehrxf::terminology::ehn;
use eehrxf::terminology::translation::MYHEALTH_EU_CORE;
use eehrxf::terminology::translation::MYHEALTH_EU_LABORATORY;
use eehrxf::terminology::translation::TranslationError;
use eehrxf::terminology::translation::Translator;
use fhir_types::r4::allergy_intolerance::AllergyIntolerance;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::observation::Observation;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use http::StatusCode;

use super::LOCAL;
use super::PREFERRED;
use super::Script;
use super::Scripted;
use super::coding;

type TestResult = Result<(), Box<dyn Error>>;

/// Whether `system` is preferred.
fn preferred(system: &str) -> bool {
    system == PREFERRED
}

/// The systems translated into.
const TARGETS: [&str; 1] = [PREFERRED];

/// An allergy whose code carries `codings`.
fn allergy(codings: Vec<Coding>) -> Resource {
    Resource::AllergyIntolerance(Box::new(AllergyIntolerance {
        code: Some(CodeableConcept {
            coding: codings,
            ..CodeableConcept::default()
        }),
        patient: Reference {
            reference: Some("urn:uuid:5a6b7c8d-0000-4000-8000-000000000002".into()),
            ..Reference::default()
        },
        ..AllergyIntolerance::default()
    }))
}

/// The codings of the allergy's code, each as system and code.
fn codes(resource: &Resource) -> Vec<(String, String)> {
    let Resource::AllergyIntolerance(allergy) = resource else {
        return Vec::new();
    };
    allergy
        .code
        .iter()
        .flat_map(|concept| concept.coding.iter())
        .filter_map(|coding| {
            Some((
                coding.system.as_ref()?.value.clone()?,
                coding.code.as_ref()?.value.clone()?,
            ))
        })
        .collect()
}

fn pair(system: &str, code: &str) -> (String, String) {
    (system.to_owned(), code.to_owned())
}

/// The terminology answered by `script`.
fn scripted(script: Script) -> Terminology {
    Terminology::server(Arc::new(Scripted(script)))
}

#[tokio::test]
async fn an_equivalent_match_is_added_after_the_original_coding() -> TestResult {
    let terminology = scripted(Script::Translate(vec![
        ("equivalent", PREFERRED, "P-1"),
        ("wider", PREFERRED, "P-2"),
        ("equal", PREFERRED, "P-3"),
    ]));
    let mut translator = Translator::new(&terminology, &TARGETS);
    let translated = translator
        .translate(&allergy(vec![coding(LOCAL, "L-1")]), preferred)
        .await?;
    assert_eq!(
        codes(&translated),
        [
            pair(LOCAL, "L-1"),
            pair(PREFERRED, "P-1"),
            pair(PREFERRED, "P-3")
        ],
        "FHIR R4 ConceptMapEquivalence: only equivalent and equal are a translation"
    );
    Ok(())
}

#[tokio::test]
async fn a_concept_in_a_preferred_system_is_not_asked() -> TestResult {
    let terminology = scripted(Script::Refuse(StatusCode::INTERNAL_SERVER_ERROR));
    let mut translator = Translator::new(&terminology, &TARGETS);
    let recorded = allergy(vec![coding(LOCAL, "L-1"), coding(PREFERRED, "P-1")]);
    let translated = translator.translate(&recorded, preferred).await?;
    assert_eq!(translated, recorded);
    Ok(())
}

#[tokio::test]
async fn with_no_server_the_code_is_transferred_as_recorded() -> TestResult {
    let terminology = Terminology::none();
    let mut translator = Translator::new(&terminology, &TARGETS);
    let recorded = allergy(vec![coding(LOCAL, "L-1")]);
    let translated = translator.translate(&recorded, preferred).await?;
    assert_eq!(translated, recorded, "eHN PS Art 10(7)");
    Ok(())
}

#[tokio::test]
async fn no_accepted_match_leaves_the_code_as_recorded() -> TestResult {
    let terminology = scripted(Script::Translate(vec![("narrower", PREFERRED, "P-2")]));
    let mut translator = Translator::new(&terminology, &TARGETS);
    let recorded = allergy(vec![coding(LOCAL, "L-1")]);
    let translated = translator.translate(&recorded, preferred).await?;
    assert_eq!(translated, recorded);
    Ok(())
}

#[tokio::test]
async fn a_refused_translation_fails_and_is_never_no_translation() {
    let terminology = scripted(Script::Refuse(StatusCode::SERVICE_UNAVAILABLE));
    let mut translator = Translator::new(&terminology, &TARGETS);
    let refused = translator
        .translate(&allergy(vec![coding(LOCAL, "L-1")]), preferred)
        .await;
    assert!(
        matches!(
            refused,
            Err(TranslationError::Terminology(TerminologyError::Status {
                operation: Operation::Translate,
                status: StatusCode::SERVICE_UNAVAILABLE,
            }))
        ),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_translation_records_its_original_equivalence_and_map() -> TestResult {
    // NOTE: draft EEHRxF Art 6(2) and (3): the original and the mapped coding both travel,
    // and the mapping rule names its equivalence and provenance.
    let terminology = Terminology::server(Arc::new(Scripted(Script::Translate(vec![(
        "equivalent",
        PREFERRED,
        "P-1",
    )]))));
    let mut translator = Translator::new(&terminology, &TARGETS);
    let translated = translator
        .translate(&allergy(vec![coding(LOCAL, "L-1")]), preferred)
        .await?;
    assert_eq!(
        codes(&translated),
        [pair(LOCAL, "L-1"), pair(PREFERRED, "P-1")]
    );
    let [mapping] = translator.mapped() else {
        return Err("one mapping".into());
    };
    assert_eq!(mapping.original, pair(LOCAL, "L-1"));
    assert_eq!(mapping.translated.equivalence, "equivalent");
    assert_eq!(mapping.translated.map.as_deref(), Some(super::MAP));
    assert_eq!(translator.unmapped().len(), 0);
    Ok(())
}

#[tokio::test]
async fn an_unmapped_national_code_carries_its_english_designation_and_is_counted() -> TestResult {
    // NOTE: draft EEHRxF Art 6(3): with no mapping, the national code goes out with its
    // textual designation in English.
    let terminology = Terminology::server(Arc::new(Scripted(Script::Unmapped("Peanut allergy"))));
    let mut national = coding(LOCAL, "L-1");
    national.display = Some("Pinda-allergie".into());
    let mut translator = Translator::new(&terminology, &TARGETS).for_non_english();
    let translated = translator
        .translate(&allergy(vec![national]), preferred)
        .await?;
    assert_eq!(codes(&translated), [pair(LOCAL, "L-1")]);
    assert_eq!(translator.unmapped(), [pair(LOCAL, "L-1")]);
    let json = serde_json::to_value(&translated)?;
    let coding = &json["code"]["coding"][0];
    assert_eq!(coding["display"], "Pinda-allergie");
    let translation = &coding["_display"]["extension"][0];
    assert_eq!(
        translation["url"],
        "http://hl7.org/fhir/StructureDefinition/translation"
    );
    assert_eq!(translation["extension"][0]["valueCode"], "en");
    assert_eq!(translation["extension"][1]["valueString"], "Peanut allergy");
    Ok(())
}

#[tokio::test]
async fn an_unmapped_code_with_no_display_takes_the_english_designation() -> TestResult {
    let terminology = Terminology::server(Arc::new(Scripted(Script::Unmapped("Peanut allergy"))));
    let mut translator = Translator::new(&terminology, &TARGETS).for_non_english();
    let translated = translator
        .translate(&allergy(vec![coding(LOCAL, "L-1")]), preferred)
        .await?;
    let json = serde_json::to_value(&translated)?;
    assert_eq!(json["code"]["coding"][0]["display"], "Peanut allergy");
    Ok(())
}

#[tokio::test]
async fn a_refused_terminology_call_fails_the_translation() -> TestResult {
    let terminology = Terminology::server(Arc::new(Scripted(Script::Refuse(
        StatusCode::SERVICE_UNAVAILABLE,
    ))));
    let mut translator = Translator::new(&terminology, &TARGETS).for_non_english();
    let found = translator
        .translate(&allergy(vec![coding(LOCAL, "L-1")]), preferred)
        .await;
    assert!(matches!(found, Err(TranslationError::Terminology { .. })));
    Ok(())
}

#[tokio::test]
async fn a_bound_concept_in_a_preferred_system_gains_the_binding_system() -> TestResult {
    // NOTE: MyHealth@EU Core 1.0.1 binds AllergyIntolerance.code required to
    // eHDSIAllergenWithExceptions, whose drug members are ATC codes.
    let terminology = scripted(Script::Translate(vec![("equivalent", ehn::ATC, "J01CE")]));
    let mut translator = Translator::new(&terminology, &TARGETS).with_bindings(&MYHEALTH_EU_CORE);
    let translated = translator
        .translate(&allergy(vec![coding(PREFERRED, "P-1")]), preferred)
        .await?;
    assert_eq!(
        codes(&translated),
        [pair(PREFERRED, "P-1"), pair(ehn::ATC, "J01CE")],
        "draft EEHRxF Art 6(2): the original stays and the mapped coding follows it"
    );
    let [mapping] = translator.mapped() else {
        return Err("one mapping".into());
    };
    assert_eq!(mapping.original, pair(PREFERRED, "P-1"));
    assert_eq!(mapping.translated.map.as_deref(), Some(super::MAP));
    assert_eq!(translator.unbound().len(), 0);
    assert_eq!(translator.unmapped().len(), 0);
    Ok(())
}

#[tokio::test]
async fn a_bound_concept_no_translation_reaches_is_unbound() -> TestResult {
    let terminology = scripted(Script::Translate(vec![("wider", ehn::ATC, "J01C")]));
    let mut translator = Translator::new(&terminology, &TARGETS).with_bindings(&MYHEALTH_EU_CORE);
    let recorded = allergy(vec![coding(PREFERRED, "P-1")]);
    let translated = translator.translate(&recorded, preferred).await?;
    assert_eq!(translated, recorded, "no code is invented");
    let [unbound] = translator.unbound() else {
        return Err("one unbound concept".into());
    };
    assert_eq!(unbound.binding.element, "AllergyIntolerance.code");
    assert_eq!(unbound.originals, [pair(PREFERRED, "P-1")]);
    assert_eq!(unbound.location, "AllergyIntolerance.code");
    assert_eq!(translator.unmapped().len(), 0);
    Ok(())
}

#[tokio::test]
async fn a_bound_concept_already_in_the_binding_system_is_not_asked() -> TestResult {
    let terminology = scripted(Script::Refuse(StatusCode::INTERNAL_SERVER_ERROR));
    let mut translator = Translator::new(&terminology, &TARGETS).with_bindings(&MYHEALTH_EU_CORE);
    let recorded = allergy(vec![coding(PREFERRED, "P-1"), coding(ehn::ATC, "J01CE")]);
    let translated = translator.translate(&recorded, preferred).await?;
    assert_eq!(translated, recorded);
    assert_eq!(translator.unbound().len(), 0);
    Ok(())
}

#[tokio::test]
async fn a_refused_translation_of_a_bound_concept_fails() {
    let terminology = scripted(Script::Refuse(StatusCode::SERVICE_UNAVAILABLE));
    let mut translator = Translator::new(&terminology, &TARGETS).with_bindings(&MYHEALTH_EU_CORE);
    let refused = translator
        .translate(&allergy(vec![coding(PREFERRED, "P-1")]), preferred)
        .await;
    assert!(
        matches!(
            refused,
            Err(TranslationError::Terminology(TerminologyError::Status {
                operation: Operation::Translate,
                status: StatusCode::SERVICE_UNAVAILABLE,
            }))
        ),
        "{refused:?}"
    );
}

/// The terminology whose `$translate` matches `L-1` to `P-1` with `display`,
/// and whose `$lookup` answers `designation`.
fn designating(
    display: Option<&'static str>,
    designation: Result<Option<&'static str>, StatusCode>,
) -> Terminology {
    scripted(Script::Designate {
        matches: vec![("equivalent", PREFERRED, "P-1")],
        display,
        designation,
    })
}

#[tokio::test]
async fn an_added_coding_with_no_display_takes_the_designation_in_the_requested_language()
-> TestResult {
    let terminology = designating(None, Ok(Some("Pinda")));
    let mut translator = Translator::new(&terminology, &TARGETS).with_designations("nl");
    let translated = translator
        .translate(&allergy(vec![coding(LOCAL, "L-1")]), preferred)
        .await?;
    let json = serde_json::to_value(&translated)?;
    let codings = &json["code"]["coding"];
    assert_eq!(codings[1]["code"], "P-1");
    assert_eq!(codings[1]["display"], "Pinda");
    assert!(
        codings[0].get("display").is_none(),
        "the original coding is left as recorded"
    );
    Ok(())
}

#[tokio::test]
async fn an_added_coding_with_a_display_carries_the_designation_as_its_translation() -> TestResult {
    let terminology = designating(Some("Peanut"), Ok(Some("Pinda")));
    let mut translator = Translator::new(&terminology, &TARGETS).with_designations("nl");
    let translated = translator
        .translate(&allergy(vec![coding(LOCAL, "L-1")]), preferred)
        .await?;
    let json = serde_json::to_value(&translated)?;
    let coding = &json["code"]["coding"][1];
    assert_eq!(coding["display"], "Peanut");
    let translation = &coding["_display"]["extension"][0];
    assert_eq!(
        translation["url"],
        "http://hl7.org/fhir/StructureDefinition/translation"
    );
    assert_eq!(translation["extension"][0]["valueCode"], "nl");
    assert_eq!(translation["extension"][1]["valueString"], "Pinda");
    Ok(())
}

#[tokio::test]
async fn an_added_coding_the_server_holds_no_designation_for_is_left_as_it_is() -> TestResult {
    let terminology = designating(None, Ok(None));
    let mut translator = Translator::new(&terminology, &TARGETS).with_designations("nl");
    let translated = translator
        .translate(&allergy(vec![coding(LOCAL, "L-1")]), preferred)
        .await?;
    assert_eq!(
        codes(&translated),
        [pair(LOCAL, "L-1"), pair(PREFERRED, "P-1")]
    );
    let json = serde_json::to_value(&translated)?;
    let coding = &json["code"]["coding"][1];
    assert!(coding.get("display").is_none());
    assert!(coding.get("_display").is_none());
    Ok(())
}

#[tokio::test]
async fn a_refused_designation_lookup_fails_the_translation() {
    let terminology = designating(None, Err(StatusCode::SERVICE_UNAVAILABLE));
    let mut translator = Translator::new(&terminology, &TARGETS).with_designations("nl");
    let refused = translator
        .translate(&allergy(vec![coding(LOCAL, "L-1")]), preferred)
        .await;
    assert!(
        matches!(
            refused,
            Err(TranslationError::Terminology(TerminologyError::Status {
                operation: Operation::Lookup,
                status: StatusCode::SERVICE_UNAVAILABLE,
            }))
        ),
        "{refused:?}"
    );
}

#[tokio::test]
async fn no_designation_is_asked_without_a_requested_language() -> TestResult {
    let terminology = designating(None, Err(StatusCode::SERVICE_UNAVAILABLE));
    let mut translator = Translator::new(&terminology, &TARGETS);
    let translated = translator
        .translate(&allergy(vec![coding(LOCAL, "L-1")]), preferred)
        .await?;
    assert_eq!(
        codes(&translated),
        [pair(LOCAL, "L-1"), pair(PREFERRED, "P-1")]
    );
    Ok(())
}

/// A laboratory result whose code carries `codings`.
fn result(codings: Vec<Coding>) -> Resource {
    Resource::Observation(Box::new(Observation {
        status: "final".into(),
        code: CodeableConcept {
            coding: codings,
            ..CodeableConcept::default()
        },
        ..Observation::default()
    }))
}

/// The codings of the result's code, each as system and code.
fn result_codes(resource: &Resource) -> Vec<(String, String)> {
    let Resource::Observation(result) = resource else {
        return Vec::new();
    };
    result
        .code
        .coding
        .iter()
        .filter_map(|coding| {
            Some((
                coding.system.as_ref()?.value.clone()?,
                coding.code.as_ref()?.value.clone()?,
            ))
        })
        .collect()
}

#[tokio::test]
async fn a_laboratory_result_recorded_in_npu_gains_its_loinc_code() -> TestResult {
    // NOTE: MyHealth@EU Laboratory 9.2.0 binds Observation.code required to
    // eHDSILabCodeWithExceptions, whose members are LOINC codes.
    let terminology = scripted(Script::Translate(vec![(
        "equivalent",
        ehn::LOINC,
        "2345-7",
    )]));
    let mut translator =
        Translator::new(&terminology, &TARGETS).with_bindings(&MYHEALTH_EU_LABORATORY);
    let translated = translator
        .translate(&result(vec![coding(ehn::NPU, "NPU02192")]), |system| {
            system == ehn::NPU
        })
        .await?;
    assert_eq!(
        result_codes(&translated),
        [pair(ehn::NPU, "NPU02192"), pair(ehn::LOINC, "2345-7")],
        "draft EEHRxF Art 6(2): the original stays and the mapped coding follows it"
    );
    assert_eq!(translator.unbound().len(), 0);
    Ok(())
}

#[tokio::test]
async fn a_laboratory_result_already_in_loinc_is_not_asked() -> TestResult {
    let terminology = scripted(Script::Refuse(StatusCode::INTERNAL_SERVER_ERROR));
    let mut translator =
        Translator::new(&terminology, &TARGETS).with_bindings(&MYHEALTH_EU_LABORATORY);
    let recorded = result(vec![coding(ehn::LOINC, "2345-7")]);
    let translated = translator
        .translate(&recorded, |system| system == ehn::LOINC)
        .await?;
    assert_eq!(translated, recorded);
    Ok(())
}

#[tokio::test]
async fn a_laboratory_result_no_loinc_translation_reaches_is_unbound() -> TestResult {
    let terminology = scripted(Script::Translate(vec![("wider", ehn::LOINC, "2345")]));
    let mut translator =
        Translator::new(&terminology, &TARGETS).with_bindings(&MYHEALTH_EU_LABORATORY);
    let recorded = result(vec![coding(ehn::NPU, "NPU02192")]);
    let translated = translator
        .translate(&recorded, |system| system == ehn::NPU)
        .await?;
    assert_eq!(translated, recorded, "no code is invented");
    let [unbound] = translator.unbound() else {
        return Err("one unbound concept".into());
    };
    assert_eq!(unbound.binding.element, "Observation.code");
    assert_eq!(unbound.binding.systems, [ehn::LOINC]);
    Ok(())
}
