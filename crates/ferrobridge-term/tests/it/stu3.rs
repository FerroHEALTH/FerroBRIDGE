// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The three operations and the batch in FHIR STU3, over the STU3
//! `OperationDefinition-CodeSystem-lookup`, `-ConceptMap-translate` and
//! `-ValueSet-validate-code` (`hl7.fhir.r3.core` 3.0.2;
//! <https://hl7.org/fhir/STU3/terminology-service.html>).
//!
//! Every answer is a synthetic STU3 `Parameters` shaped after those
//! definitions, and every parameter STU3 lacks is refused before a request
//! leaves the client.

use crate::support::{client, parameter, release_path, request_body, request_header};
use ferrobridge_term::client::FHIR_JSON;
use ferrobridge_term::concept::{Concept, Equivalence};
use ferrobridge_term::config::WireVersion;
use ferrobridge_term::error::{BodyError, Error};
use ferrobridge_term::outcome::{
    BatchOutcome, Coded, LookupOutcome, Request, TranslateOutcome, Translation, ValidateOutcome,
    Validation,
};
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::stubs::terminology;
use std::error::Error as StdError;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The release under test.
const STU3: WireVersion = WireVersion::Stu3;

/// Mounts `response` as the one answer of `operation` under the STU3 prefix.
async fn mount(server: &MockServer, operation: &str, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(format!("/{}/{operation}", release_path(STU3))))
        .respond_with(response)
        .mount(server)
        .await;
}

/// Mounts `response` as the one answer of the STU3 service base.
async fn mount_base(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(format!("/{}", release_path(STU3))))
        .respond_with(response)
        .mount(server)
        .await;
}

/// Returns how many requests `server` received.
async fn received(server: &MockServer) -> Result<usize, Box<dyn StdError>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("the mock server is not recording requests")?
        .len())
}

/// Returns a `200` carrying `parameters` as an STU3 `Parameters`.
fn parameters(parameters: &[serde_json::Value]) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("Content-Type", FHIR_JSON)
        .set_body_string(terminology::parameters_resource(parameters).to_string())
}

/// Returns the member validation of the fixture value set, with `coded`.
fn validation(coded: Coded) -> Validation {
    Validation {
        value_set_url: fixtures::TERM_VALUE_SET_URL.to_owned(),
        value_set_version: None,
        date: None,
        coded,
    }
}

/// Returns the fixture member code as a `Coding`.
fn member() -> Concept {
    Concept {
        system: Some(fixtures::TERM_SOURCE_SYSTEM.to_owned()),
        version: Some("1.0.0".to_owned()),
        code: Some(fixtures::TERM_MEMBER_CODE.to_owned()),
        display: Some(fixtures::TERM_MEMBER_DISPLAY.to_owned()),
    }
}

#[tokio::test]
async fn a_lookup_sends_stu3_parameters_and_reads_a_code_property() -> Result<(), Box<dyn StdError>>
{
    let server = MockServer::start().await;
    mount(
        &server,
        "CodeSystem/$lookup",
        terminology::lookup_detailed(
            "FerroBridgeSyntheticSource",
            fixtures::TERM_MEMBER_DISPLAY,
            Some("1.0.0"),
            &[("nl", "Alfa bevinding")],
            &[("parent", "valueCode", serde_json::json!("root"))],
        ),
    )
    .await;

    let outcome = client(&server, STU3)?
        .lookup(
            fixtures::TERM_SOURCE_SYSTEM,
            fixtures::TERM_MEMBER_CODE,
            Some("1.0.0"),
        )
        .await?;

    let LookupOutcome::Found {
        display,
        designations,
        properties,
        ..
    } = &outcome
    else {
        return Err(format!("expected a lookup answer, got {outcome:?}").into());
    };
    assert_eq!(fixtures::TERM_MEMBER_DISPLAY, display);
    assert_eq!(1, designations.len());
    let value = properties
        .first()
        .and_then(|property| property.value.as_ref())
        .ok_or("the property carries its value")?;
    assert_eq!("valueCode", value.member);
    assert_eq!(Some("root"), value.as_str());

    let body = request_body(&server, 0).await?;
    assert_eq!(Some("Parameters"), body["resourceType"].as_str());
    assert_eq!(
        Some(("valueUri".to_owned(), fixtures::TERM_SOURCE_SYSTEM.into())),
        parameter(&body, "system")
    );
    assert_eq!(
        Some(("valueString".to_owned(), "1.0.0".into())),
        parameter(&body, "version")
    );
    // NOTE: STU3 names `application/fhir+json` its JSON media type and defines no
    // `fhirVersion` parameter on it (<https://hl7.org/fhir/STU3/http.html#mime-type>).
    assert_eq!(
        vec![FHIR_JSON],
        request_header(&server, 0, "content-type").await?
    );
    assert_eq!(vec![FHIR_JSON], request_header(&server, 0, "accept").await?);
    Ok(())
}

#[tokio::test]
async fn an_stu3_lookup_property_that_is_not_a_code_is_a_body_error()
-> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount(
        &server,
        "CodeSystem/$lookup",
        terminology::lookup_detailed(
            "FerroBridgeSyntheticSource",
            fixtures::TERM_MEMBER_DISPLAY,
            None,
            &[],
            &[("weight", "valueDecimal", serde_json::json!(1.5))],
        ),
    )
    .await;

    let error = client(&server, STU3)?
        .lookup(
            fixtures::TERM_SOURCE_SYSTEM,
            fixtures::TERM_MEMBER_CODE,
            None,
        )
        .await
        .expect_err("STU3 types property.value as code");
    let Error::Body { source, .. } = &error else {
        return Err(format!("expected a body error, got {error:?}").into());
    };
    assert!(
        matches!(source.as_ref(), BodyError::Parameters(_)),
        "{source:?}"
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_code_is_not_found_in_stu3() -> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount(
        &server,
        "CodeSystem/$lookup",
        terminology::outcome(
            404,
            "not-found",
            Some("not-found"),
            "the code is not in the code system",
        ),
    )
    .await;

    let outcome = client(&server, STU3)?
        .lookup(
            fixtures::TERM_SOURCE_SYSTEM,
            fixtures::TERM_UNKNOWN_CODE,
            None,
        )
        .await?;
    let LookupOutcome::NotFound { outcome } = &outcome else {
        return Err(format!("expected not found, got {outcome:?}").into());
    };
    assert!(outcome.has_tx_issue("not-found"));
    Ok(())
}

#[tokio::test]
async fn a_translation_reads_the_stu3_match_parts() -> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount(
        &server,
        "ConceptMap/$translate",
        parameters(&[
            serde_json::json!({"name": "result", "valueBoolean": true}),
            serde_json::json!({
                "name": "match",
                "part": [
                    {"name": "equivalence", "valueCode": "equivalent"},
                    {"name": "concept", "valueCoding": {
                        "system": fixtures::TERM_TARGET_SYSTEM,
                        "code": fixtures::TERM_MEMBER_TARGET_CODE,
                        "display": "Target alpha",
                    }},
                    {"name": "source", "valueUri": fixtures::TERM_CONCEPT_MAP_URL},
                ],
            }),
            serde_json::json!({
                "name": "match",
                "part": [
                    {"name": "equivalence", "valueCode": "wider"},
                    {"name": "concept", "valueCoding": {
                        "system": fixtures::TERM_TARGET_SYSTEM,
                        "code": "B1",
                    }},
                ],
            }),
        ]),
    )
    .await;

    let outcome = client(&server, STU3)?
        .translate(
            &Translation::new(fixtures::TERM_SOURCE_SYSTEM, fixtures::TERM_MEMBER_CODE)
                .with_target_system(fixtures::TERM_TARGET_SYSTEM),
        )
        .await?;
    let TranslateOutcome::Translated { matches, .. } = &outcome else {
        return Err(format!("expected a translation, got {outcome:?}").into());
    };
    assert_eq!(
        vec![Some(Equivalence::Equivalent), Some(Equivalence::Wider)],
        matches
            .iter()
            .map(|found| found.equivalence.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        Some(fixtures::TERM_CONCEPT_MAP_URL),
        matches.first().and_then(|found| found.source.as_deref())
    );
    assert_eq!(1, outcome.accepted().count());

    let body = request_body(&server, 0).await?;
    assert_eq!(
        Some(("valueUri".to_owned(), fixtures::TERM_TARGET_SYSTEM.into())),
        parameter(&body, "targetsystem")
    );
    assert_eq!(None, parameter(&body, "url"));
    Ok(())
}

#[tokio::test]
async fn an_stu3_translation_with_no_match_is_no_match() -> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount(
        &server,
        "ConceptMap/$translate",
        terminology::translate_no_match("no mapping for the code"),
    )
    .await;

    let outcome = client(&server, STU3)?
        .translate(
            &Translation::new(fixtures::TERM_SOURCE_SYSTEM, fixtures::TERM_NON_MEMBER_CODE)
                .with_target_system(fixtures::TERM_TARGET_SYSTEM),
        )
        .await?;
    assert!(
        matches!(&outcome, TranslateOutcome::NoMatch { message: Some(text) } if text == "no mapping for the code"),
        "{outcome:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_concept_map_url_is_refused_in_stu3_and_nothing_is_sent() -> Result<(), Box<dyn StdError>>
{
    let server = MockServer::start().await;
    let error = client(&server, STU3)?
        .translate(
            &Translation::new(fixtures::TERM_SOURCE_SYSTEM, fixtures::TERM_MEMBER_CODE)
                .with_target_system(fixtures::TERM_TARGET_SYSTEM)
                .with_concept_map(fixtures::TERM_CONCEPT_MAP_URL),
        )
        .await
        .expect_err("STU3 $translate has no url parameter");
    assert!(
        matches!(
            &error,
            Error::UnsupportedParameter {
                operation: "ConceptMap/$translate",
                parameter: "url",
                wire_version: WireVersion::Stu3,
            }
        ),
        "{error:?}"
    );
    assert_eq!("unsupported-parameter", error.kind());
    assert!(!error.is_retryable());
    assert!(error.to_string().contains("STU3"), "{error}");
    assert_eq!(0, received(&server).await?);
    Ok(())
}

#[tokio::test]
async fn a_code_sends_its_system_version_as_the_stu3_version() -> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount(
        &server,
        "ValueSet/$validate-code",
        terminology::validate_code(true, Some(fixtures::TERM_MEMBER_DISPLAY), None),
    )
    .await;

    let outcome = client(&server, STU3)?
        .validate(&validation(Coded::Code {
            system: fixtures::TERM_SOURCE_SYSTEM.to_owned(),
            code: fixtures::TERM_MEMBER_CODE.to_owned(),
            version: Some("1.0.0".to_owned()),
            display: Some(fixtures::TERM_MEMBER_DISPLAY.to_owned()),
        }))
        .await?;
    assert!(outcome.is_valid());

    let body = request_body(&server, 0).await?;
    assert_eq!(
        Some(("valueUri".to_owned(), fixtures::TERM_VALUE_SET_URL.into())),
        parameter(&body, "url")
    );
    assert_eq!(
        Some(("valueString".to_owned(), "1.0.0".into())),
        parameter(&body, "version")
    );
    assert_eq!(None, parameter(&body, "systemVersion"));
    assert_eq!(None, parameter(&body, "valueSetVersion"));
    Ok(())
}

#[tokio::test]
async fn a_coding_and_a_codeable_concept_go_as_their_stu3_parameters()
-> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount(
        &server,
        "ValueSet/$validate-code",
        terminology::validate_code(true, None, None),
    )
    .await;
    let client = client(&server, STU3)?;

    client
        .validate(&validation(Coded::Coding(member())))
        .await?;
    client
        .validate(&validation(Coded::CodeableConcept {
            coding: vec![member()],
            text: Some("alpha".to_owned()),
        }))
        .await?;

    let coding = request_body(&server, 0).await?;
    let (member_name, value) = parameter(&coding, "coding").ok_or("the coding parameter")?;
    assert_eq!("valueCoding", member_name);
    assert_eq!(Some(fixtures::TERM_MEMBER_CODE), value["code"].as_str());
    assert_eq!(Some("1.0.0"), value["version"].as_str());

    let concept = request_body(&server, 1).await?;
    let (member_name, value) =
        parameter(&concept, "codeableConcept").ok_or("the codeableConcept parameter")?;
    assert_eq!("valueCodeableConcept", member_name);
    assert_eq!(Some("alpha"), value["text"].as_str());
    assert_eq!(
        Some(fixtures::TERM_MEMBER_CODE),
        value["coding"][0]["code"].as_str()
    );
    Ok(())
}

#[tokio::test]
async fn a_false_stu3_validation_is_invalid_with_its_message() -> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount(
        &server,
        "ValueSet/$validate-code",
        terminology::validate_code(false, None, Some("the code is not in the value set")),
    )
    .await;

    let outcome = client(&server, STU3)?
        .validate_code(
            fixtures::TERM_VALUE_SET_URL,
            fixtures::TERM_SOURCE_SYSTEM,
            fixtures::TERM_NON_MEMBER_CODE,
        )
        .await?;
    let ValidateOutcome::Invalid { message, outcome } = &outcome else {
        return Err(format!("expected an invalid code, got {outcome:?}").into());
    };
    assert_eq!(Some("the code is not in the value set"), message.as_deref());
    assert!(outcome.is_none(), "STU3 declares no issues parameter");
    Ok(())
}

#[tokio::test]
async fn an_stu3_answer_with_an_undeclared_issues_parameter_is_a_body_error()
-> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount(
        &server,
        "ValueSet/$validate-code",
        terminology::validate_code_invalid("the code is not in the value set", "invalid-code"),
    )
    .await;

    let error = client(&server, STU3)?
        .validate_code(
            fixtures::TERM_VALUE_SET_URL,
            fixtures::TERM_SOURCE_SYSTEM,
            fixtures::TERM_NON_MEMBER_CODE,
        )
        .await
        .expect_err("STU3 $validate-code declares no issues out parameter");
    let Error::Body { source, .. } = &error else {
        return Err(format!("expected a body error, got {error:?}").into());
    };
    assert!(
        matches!(source.as_ref(), BodyError::Parameters(_)),
        "{source:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_value_set_version_is_refused_in_stu3_and_nothing_is_sent()
-> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    let mut asked = validation(Coded::Coding(member()));
    asked.value_set_version = Some("2.0.0".to_owned());

    let error = client(&server, STU3)?
        .validate(&asked)
        .await
        .expect_err("STU3 $validate-code has no valueSetVersion");
    assert!(
        matches!(
            &error,
            Error::UnsupportedParameter {
                operation: "ValueSet/$validate-code",
                parameter: "valueSetVersion",
                wire_version: WireVersion::Stu3,
            }
        ),
        "{error:?}"
    );
    assert_eq!(0, received(&server).await?);
    Ok(())
}

#[tokio::test]
async fn an_stu3_batch_answers_each_entry_by_position() -> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount_base(
        &server,
        terminology::batch_response(&[
            (
                "200 OK",
                terminology::parameters_resource(&terminology::lookup_body(
                    "FerroBridgeSyntheticSource",
                    fixtures::TERM_MEMBER_DISPLAY,
                    None,
                    &[],
                    &[],
                )),
            ),
            (
                "200 OK",
                terminology::parameters_resource(&terminology::translate_body(&[(
                    "equal",
                    fixtures::TERM_TARGET_SYSTEM,
                    fixtures::TERM_MEMBER_TARGET_CODE,
                    "Target alpha",
                )])),
            ),
            (
                "200 OK",
                terminology::parameters_resource(&terminology::validate_code_body(
                    false,
                    None,
                    Some("the code is not in the value set"),
                    None,
                )),
            ),
        ]),
    )
    .await;

    let requests = vec![
        Request::Lookup {
            system: fixtures::TERM_SOURCE_SYSTEM.to_owned(),
            code: fixtures::TERM_MEMBER_CODE.to_owned(),
            version: None,
            date: None,
        },
        Request::Translate(
            Translation::new(fixtures::TERM_SOURCE_SYSTEM, fixtures::TERM_MEMBER_CODE)
                .with_target_system(fixtures::TERM_TARGET_SYSTEM),
        ),
        Request::ValidateCode(validation(Coded::Code {
            system: fixtures::TERM_SOURCE_SYSTEM.to_owned(),
            code: fixtures::TERM_NON_MEMBER_CODE.to_owned(),
            version: Some("1.0.0".to_owned()),
            display: None,
        })),
    ];
    let answers = client(&server, STU3)?.batch(&requests).await?;
    assert_eq!(3, answers.len());
    assert!(matches!(
        answers.first(),
        Some(Ok(BatchOutcome::Lookup(LookupOutcome::Found { .. })))
    ));
    let Some(Ok(BatchOutcome::Translate(translated))) = answers.get(1) else {
        return Err(format!("expected a translation, got {:?}", answers.get(1)).into());
    };
    assert_eq!(1, translated.accepted().count());
    assert!(matches!(
        answers.get(2),
        Some(Ok(BatchOutcome::ValidateCode(
            ValidateOutcome::Invalid { .. }
        )))
    ));

    let body = request_body(&server, 0).await?;
    assert_eq!(Some("batch"), body["type"].as_str());
    let validate = &body["entry"][2];
    assert_eq!(Some("POST"), validate["request"]["method"].as_str());
    assert_eq!(
        Some("ValueSet/$validate-code"),
        validate["request"]["url"].as_str()
    );
    assert_eq!(
        Some(("valueString".to_owned(), "1.0.0".into())),
        parameter(&validate["resource"], "version")
    );
    Ok(())
}

#[tokio::test]
async fn an_stu3_batch_with_an_unsupported_parameter_sends_nothing() -> Result<(), Box<dyn StdError>>
{
    let server = MockServer::start().await;
    let requests = vec![
        Request::Lookup {
            system: fixtures::TERM_SOURCE_SYSTEM.to_owned(),
            code: fixtures::TERM_MEMBER_CODE.to_owned(),
            version: None,
            date: None,
        },
        Request::Translate(
            Translation::new(fixtures::TERM_SOURCE_SYSTEM, fixtures::TERM_MEMBER_CODE)
                .with_target_system(fixtures::TERM_TARGET_SYSTEM)
                .with_concept_map(fixtures::TERM_CONCEPT_MAP_URL),
        ),
    ];

    let error = client(&server, STU3)?
        .batch(&requests)
        .await
        .expect_err("the translate entry names a concept map");
    assert!(
        matches!(
            &error,
            Error::UnsupportedParameter {
                operation: "ConceptMap/$translate",
                parameter: "url",
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(0, received(&server).await?);
    Ok(())
}

#[tokio::test]
async fn the_same_requests_are_sent_unchanged_in_r4() -> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/{}/ValueSet/$validate-code",
            release_path(WireVersion::R4)
        )))
        .respond_with(terminology::validate_code(true, None, None))
        .mount(&server)
        .await;
    let mut asked = validation(Coded::Code {
        system: fixtures::TERM_SOURCE_SYSTEM.to_owned(),
        code: fixtures::TERM_MEMBER_CODE.to_owned(),
        version: Some("1.0.0".to_owned()),
        display: None,
    });
    asked.value_set_version = Some("2.0.0".to_owned());

    client(&server, WireVersion::R4)?.validate(&asked).await?;

    let body = request_body(&server, 0).await?;
    assert_eq!(
        Some(("valueString".to_owned(), "2.0.0".into())),
        parameter(&body, "valueSetVersion")
    );
    assert_eq!(
        Some(("valueString".to_owned(), "1.0.0".into())),
        parameter(&body, "systemVersion")
    );
    Ok(())
}
#[tokio::test]
async fn check_names_the_parameter_stu3_lacks_and_passes_the_rest() -> Result<(), Box<dyn StdError>>
{
    let server = MockServer::start().await;
    let stu3 = client(&server, STU3)?;
    let mut versioned = validation(Coded::Coding(member()));
    versioned.value_set_version = Some("2.0.0".to_owned());
    let error = stu3
        .check(&Request::ValidateCode(versioned.clone()))
        .expect_err("STU3 $validate-code has no valueSetVersion");
    assert!(
        matches!(
            &error,
            Error::UnsupportedParameter {
                operation: "ValueSet/$validate-code",
                parameter: "valueSetVersion",
                wire_version: WireVersion::Stu3,
            }
        ),
        "{error:?}"
    );
    let mapped = Translation::new("http://example.org/a", "x")
        .with_concept_map("http://example.org/ConceptMap/m");
    assert!(stu3.check(&Request::Translate(mapped.clone())).is_err());
    assert!(
        stu3.check(&Request::ValidateCode(validation(Coded::Coding(member()))))
            .is_ok()
    );
    let r4 = client(&server, WireVersion::R4)?;
    assert!(r4.check(&Request::ValidateCode(versioned)).is_ok());
    assert!(r4.check(&Request::Translate(mapped)).is_ok());
    assert_eq!(0, received(&server).await?);
    Ok(())
}
