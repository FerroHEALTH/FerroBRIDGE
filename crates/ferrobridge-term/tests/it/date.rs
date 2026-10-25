// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `date` input of `CodeSystem/$lookup` and `ValueSet/$validate-code`,
//! which R4 declares `0..1` on both
//! (<https://hl7.org/fhir/R4/codesystem-operation-lookup.html>,
//! <https://hl7.org/fhir/R4/valueset-operation-validate-code.html>).

use crate::support::{RELEASES, client, parameter, release_path, request_body};
use ferrobridge_term::config::WireVersion;
use ferrobridge_term::outcome::{Coded, Validation};
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::stubs::terminology;
use std::error::Error;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The date every case asks for.
const DATE: &str = "2025-06-30T12:00:00Z";

/// Mounts `response` as the one answer of `operation` under `version`.
async fn mount(
    server: &MockServer,
    version: WireVersion,
    operation: &str,
    response: ResponseTemplate,
) {
    Mock::given(method("POST"))
        .and(path(format!("/{}/{operation}", release_path(version))))
        .respond_with(response)
        .mount(server)
        .await;
}

/// The `date` parameter as the request carries it.
fn date() -> (String, serde_json::Value) {
    ("valueDateTime".to_owned(), serde_json::Value::from(DATE))
}

#[tokio::test]
async fn a_lookup_at_a_date_sends_it() -> Result<(), Box<dyn Error>> {
    for version in [WireVersion::Stu3, WireVersion::R4, WireVersion::R4B] {
        let server = MockServer::start().await;
        mount(
            &server,
            version,
            ferrobridge_term::LOOKUP,
            terminology::lookup(
                "FerroBridgeSyntheticSource",
                fixtures::TERM_MEMBER_DISPLAY,
                None,
            ),
        )
        .await;
        client(&server, version)?
            .lookup_at(
                fixtures::TERM_SOURCE_SYSTEM,
                fixtures::TERM_MEMBER_CODE,
                None,
                DATE,
            )
            .await?;
        assert_eq!(
            Some(date()),
            parameter(&request_body(&server, 0).await?, "date"),
            "{}",
            version.as_str()
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_lookup_without_a_date_sends_none() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount(
        &server,
        WireVersion::R4,
        ferrobridge_term::LOOKUP,
        terminology::lookup(
            "FerroBridgeSyntheticSource",
            fixtures::TERM_MEMBER_DISPLAY,
            None,
        ),
    )
    .await;
    client(&server, WireVersion::R4)?
        .lookup(
            fixtures::TERM_SOURCE_SYSTEM,
            fixtures::TERM_MEMBER_CODE,
            None,
        )
        .await?;
    assert_eq!(None, parameter(&request_body(&server, 0).await?, "date"));
    Ok(())
}

#[tokio::test]
async fn a_validation_at_a_date_sends_it() -> Result<(), Box<dyn Error>> {
    for version in [WireVersion::Stu3, WireVersion::R4, WireVersion::R4B] {
        let server = MockServer::start().await;
        mount(
            &server,
            version,
            ferrobridge_term::VALIDATE_CODE,
            terminology::validate_code(true, None, None),
        )
        .await;
        let validation = Validation {
            value_set_url: fixtures::TERM_VALUE_SET_URL.to_owned(),
            value_set_version: None,
            date: Some(DATE.to_owned()),
            coded: Coded::Code {
                system: fixtures::TERM_SOURCE_SYSTEM.to_owned(),
                code: fixtures::TERM_MEMBER_CODE.to_owned(),
                version: None,
                display: None,
            },
        };
        let outcome = client(&server, version)?.validate(&validation).await?;
        assert!(outcome.is_valid());
        assert_eq!(
            Some(date()),
            parameter(&request_body(&server, 0).await?, "date"),
            "{}",
            version.as_str()
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_validation_without_a_date_sends_none() -> Result<(), Box<dyn Error>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount(
            &server,
            version,
            ferrobridge_term::VALIDATE_CODE,
            terminology::validate_code(true, None, None),
        )
        .await;
        client(&server, version)?
            .validate_code(
                fixtures::TERM_VALUE_SET_URL,
                fixtures::TERM_SOURCE_SYSTEM,
                fixtures::TERM_MEMBER_CODE,
            )
            .await?;
        assert_eq!(None, parameter(&request_body(&server, 0).await?, "date"));
    }
    Ok(())
}
