// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The three input forms of `ValueSet/$validate-code` (a code with its
//! system, version and display, a `Coding`, a `CodeableConcept`) and the
//! several-validation form that goes as one batch
//! (<https://hl7.org/fhir/R4/valueset-operation-validate-code.html>).

use crate::support::{RELEASES, client, parameter, release_path, request_body};
use ferrobridge_term::concept::Concept;
use ferrobridge_term::config::WireVersion;
use ferrobridge_term::error::Error;
use ferrobridge_term::outcome::{Coded, ValidateOutcome, Validation};
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::stubs::terminology;
use http::StatusCode;
use serde_json::json;
use std::error::Error as StdError;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mounts `response` on the `$validate-code` route of `version`.
async fn mount_operation(server: &MockServer, version: WireVersion, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(format!(
            "/{}/ValueSet/$validate-code",
            release_path(version)
        )))
        .respond_with(response)
        .mount(server)
        .await;
}

/// Mounts `response` on the service base of `version`, where a batch goes.
async fn mount_base(server: &MockServer, version: WireVersion, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(format!("/{}", release_path(version))))
        .respond_with(response)
        .mount(server)
        .await;
}

/// The fixture member as a `Coding` with a version and a display.
fn member() -> Concept {
    Concept {
        system: Some(fixtures::TERM_SOURCE_SYSTEM.to_owned()),
        version: Some("1.0.0".to_owned()),
        code: Some(fixtures::TERM_MEMBER_CODE.to_owned()),
        display: Some(fixtures::TERM_MEMBER_DISPLAY.to_owned()),
    }
}

/// A validation of `coded` against the fixture value set.
fn validation(coded: Coded) -> Validation {
    Validation {
        value_set_url: fixtures::TERM_VALUE_SET_URL.to_owned(),
        value_set_version: None,
        date: None,
        coded,
    }
}

#[tokio::test]
async fn a_coding_goes_as_the_coding_parameter_in_both_releases() -> Result<(), Box<dyn StdError>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount_operation(
            &server,
            version,
            terminology::validate_code(true, None, None),
        )
        .await;
        let outcome = client(&server, version)?
            .validate(&validation(Coded::Coding(member())))
            .await?;
        assert!(outcome.is_valid());

        let body = request_body(&server, 0).await?;
        assert_eq!(
            Some((
                "valueCoding".to_owned(),
                json!({
                    "system": fixtures::TERM_SOURCE_SYSTEM,
                    "version": "1.0.0",
                    "code": fixtures::TERM_MEMBER_CODE,
                    "display": fixtures::TERM_MEMBER_DISPLAY
                })
            )),
            parameter(&body, "coding")
        );
        assert_eq!(None, parameter(&body, "code"), "one form per request");
    }
    Ok(())
}

#[tokio::test]
async fn a_codeable_concept_goes_with_every_coding_and_its_text() -> Result<(), Box<dyn StdError>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount_operation(
            &server,
            version,
            terminology::validate_code(true, None, None),
        )
        .await;
        let other = Concept {
            system: Some(fixtures::TERM_TARGET_SYSTEM.to_owned()),
            code: Some(fixtures::TERM_MEMBER_TARGET_CODE.to_owned()),
            ..Concept::default()
        };
        let coded = Coded::CodeableConcept {
            coding: vec![member(), other],
            text: Some("synthetic text".to_owned()),
        };
        client(&server, version)?
            .validate(&validation(coded))
            .await?;

        let body = request_body(&server, 0).await?;
        let (member_name, value) =
            parameter(&body, "codeableConcept").ok_or("no codeableConcept parameter")?;
        assert_eq!("valueCodeableConcept", member_name);
        assert_eq!(Some(&json!("synthetic text")), value.get("text"));
        assert_eq!(
            Some(2),
            value
                .get("coding")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_code_carries_its_system_version_display_and_the_value_set_version()
-> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount_operation(
        &server,
        WireVersion::R4,
        terminology::validate_code(true, None, None),
    )
    .await;
    let mut asked = validation(Coded::Code {
        system: fixtures::TERM_SOURCE_SYSTEM.to_owned(),
        code: fixtures::TERM_MEMBER_CODE.to_owned(),
        version: Some("1.0.0".to_owned()),
        display: Some(fixtures::TERM_MEMBER_DISPLAY.to_owned()),
    });
    asked.value_set_version = Some("2.0.0".to_owned());
    client(&server, WireVersion::R4)?.validate(&asked).await?;

    let body = request_body(&server, 0).await?;
    assert_eq!(
        Some(("valueString".to_owned(), json!("1.0.0"))),
        parameter(&body, "systemVersion")
    );
    assert_eq!(
        Some((
            "valueString".to_owned(),
            json!(fixtures::TERM_MEMBER_DISPLAY)
        )),
        parameter(&body, "display")
    );
    assert_eq!(
        Some(("valueString".to_owned(), json!("2.0.0"))),
        parameter(&body, "valueSetVersion")
    );
    Ok(())
}

#[tokio::test]
async fn one_validation_goes_as_one_call() -> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount_operation(
        &server,
        WireVersion::R4,
        terminology::validate_code(false, None, Some("not a member")),
    )
    .await;
    let answers = client(&server, WireVersion::R4)?
        .validate_all(&[validation(Coded::Coding(member()))])
        .await?;
    let [Ok(ValidateOutcome::Invalid { message, .. })] = answers.as_slice() else {
        return Err(format!("expected one invalid answer, got {answers:?}").into());
    };
    assert_eq!(Some("not a member"), message.as_deref());
    Ok(())
}

#[tokio::test]
async fn several_validations_go_as_one_batch_answered_by_position() -> Result<(), Box<dyn StdError>>
{
    for version in RELEASES {
        let server = MockServer::start().await;
        mount_base(
            &server,
            version,
            terminology::batch_response(&[
                (
                    "200 OK",
                    terminology::parameters_resource(&terminology::validate_code_body(
                        true,
                        Some(fixtures::TERM_MEMBER_DISPLAY),
                        None,
                        None,
                    )),
                ),
                (
                    "404 Not Found",
                    terminology::outcome_body("not-found", Some("not-found"), "no such value set"),
                ),
            ]),
        )
        .await;
        let answers = client(&server, version)?
            .validate_all(&[
                validation(Coded::Coding(member())),
                validation(Coded::Coding(member())),
            ])
            .await?;
        let [first, second] = answers.as_slice() else {
            return Err(format!("expected two answers, got {answers:?}").into());
        };
        assert!(
            matches!(first, Ok(ValidateOutcome::Valid { .. })),
            "{version:?}: {first:?}"
        );
        let Err(Error::Refused { upstream, .. }) = second else {
            return Err(
                format!("{version:?}: the second entry is not a refusal: {second:?}").into(),
            );
        };
        assert_eq!(StatusCode::NOT_FOUND, upstream.status());
        assert!(upstream.has_tx_issue("not-found"));
        let body = request_body(&server, 0).await?;
        assert_eq!(Some(&json!("batch")), body.get("type"));
    }
    Ok(())
}

#[tokio::test]
async fn a_failed_batch_is_a_typed_error_carrying_the_upstream_status()
-> Result<(), Box<dyn StdError>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount_base(
            &server,
            version,
            terminology::outcome(503, "transient", None, "the server is down"),
        )
        .await;
        let error = client(&server, version)?
            .validate_all(&[
                validation(Coded::Coding(member())),
                validation(Coded::Coding(member())),
            ])
            .await
            .expect_err("a 503 is never an answer");
        assert!(
            matches!(error, Error::ServerFailure { .. }),
            "{version:?}: {error:?}"
        );
        assert_eq!(
            Some(StatusCode::SERVICE_UNAVAILABLE),
            error.upstream_status()
        );
    }
    Ok(())
}
