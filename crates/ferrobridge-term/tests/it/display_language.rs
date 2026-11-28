// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `displayLanguage` input of `CodeSystem/$lookup`, the language the
//! displays are wanted in, `0..1` in every release
//! (<https://hl7.org/fhir/R4/codesystem-operation-lookup.html>).

use crate::support::{client, parameter, release_path, request_body};
use ferrobridge_term::config::WireVersion;
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::stubs::terminology;
use std::error::Error;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

/// Mounts a `$lookup` answer under `version`.
async fn mount(server: &MockServer, version: WireVersion) {
    Mock::given(method("POST"))
        .and(path(format!(
            "/{}/{}",
            release_path(version),
            ferrobridge_term::LOOKUP
        )))
        .respond_with(terminology::lookup(
            "FerroBridgeSyntheticSource",
            fixtures::TERM_MEMBER_DISPLAY,
            None,
        ))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_lookup_in_a_language_sends_it_as_the_display_language() -> Result<(), Box<dyn Error>> {
    for version in [WireVersion::Stu3, WireVersion::R4, WireVersion::R4B] {
        let server = MockServer::start().await;
        mount(&server, version).await;
        client(&server, version)?
            .lookup_in(
                fixtures::TERM_SOURCE_SYSTEM,
                fixtures::TERM_MEMBER_CODE,
                None,
                "nl",
            )
            .await?;
        assert_eq!(
            Some((String::from("valueCode"), serde_json::Value::from("nl"))),
            parameter(&request_body(&server, 0).await?, "displayLanguage"),
            "{}",
            version.as_str()
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_lookup_sends_no_display_language() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount(&server, WireVersion::R4).await;
    client(&server, WireVersion::R4)?
        .lookup(
            fixtures::TERM_SOURCE_SYSTEM,
            fixtures::TERM_MEMBER_CODE,
            None,
        )
        .await?;
    assert_eq!(
        None,
        parameter(&request_body(&server, 0).await?, "displayLanguage")
    );
    Ok(())
}
