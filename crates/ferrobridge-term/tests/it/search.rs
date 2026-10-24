// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `CodeSystem` search by canonical URL with `_summary=count`
//! (<https://hl7.org/fhir/R4/search.html#summary>), which tells whether the
//! server serves a code system at all.

use crate::support::{RELEASES, client, release_path};
use ferrobridge_term::config::WireVersion;
use ferrobridge_term::error::Error;
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::stubs::terminology;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mounts `response` as the one answer of the count search of `version`.
async fn mount(server: &MockServer, version: WireVersion, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(format!("/{}/CodeSystem", release_path(version))))
        .and(query_param("url", fixtures::TERM_SOURCE_SYSTEM))
        .and(query_param("_summary", "count"))
        .respond_with(response)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_served_code_system_counts_one_in_every_release() -> Result<(), Box<dyn std::error::Error>> {
    for version in [WireVersion::Stu3, WireVersion::R4, WireVersion::R4B] {
        let server = MockServer::start().await;
        mount(&server, version, terminology::search_count(1)).await;
        let count = client(&server, version)?
            .code_system_count(fixtures::TERM_SOURCE_SYSTEM)
            .await?;
        assert_eq!(1, count, "{}", version.as_str());
    }
    Ok(())
}

#[tokio::test]
async fn an_unserved_code_system_counts_zero() -> Result<(), Box<dyn std::error::Error>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount(&server, version, terminology::search_count(0)).await;
        let count = client(&server, version)?
            .code_system_count(fixtures::TERM_SOURCE_SYSTEM)
            .await?;
        assert_eq!(0, count);
    }
    Ok(())
}

#[tokio::test]
async fn an_answer_that_is_not_a_searchset_is_a_decode_error() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let bundle =
        serde_json::json!({"resourceType": "Bundle", "type": "batch-response", "total": 1});
    mount(
        &server,
        WireVersion::R4,
        ResponseTemplate::new(200).set_body_string(bundle.to_string()),
    )
    .await;

    let error = client(&server, WireVersion::R4)?
        .code_system_count(fixtures::TERM_SOURCE_SYSTEM)
        .await
        .expect_err("a batch-response is not a search answer");
    assert_eq!("body", error.kind());
    Ok(())
}

#[tokio::test]
async fn a_searchset_with_no_total_is_a_decode_error() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let bundle = serde_json::json!({"resourceType": "Bundle", "type": "searchset"});
    mount(
        &server,
        WireVersion::R4,
        ResponseTemplate::new(200).set_body_string(bundle.to_string()),
    )
    .await;

    let error = client(&server, WireVersion::R4)?
        .code_system_count(fixtures::TERM_SOURCE_SYSTEM)
        .await
        .expect_err("a count search that states no total answers nothing");
    assert_eq!("body", error.kind());
    Ok(())
}

#[tokio::test]
async fn a_refused_search_is_a_typed_error() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount(
        &server,
        WireVersion::R4,
        terminology::outcome(400, "not-supported", None, "the search is not supported"),
    )
    .await;

    let error = client(&server, WireVersion::R4)?
        .code_system_count(fixtures::TERM_SOURCE_SYSTEM)
        .await
        .expect_err("a 400 is a refusal");
    let Error::Refused {
        operation,
        upstream,
        ..
    } = &error
    else {
        return Err(format!("expected a refusal, got {error:?}").into());
    };
    assert_eq!(ferrobridge_term::CODE_SYSTEM_SEARCH, *operation);
    assert_eq!(http::StatusCode::BAD_REQUEST, upstream.status());
    Ok(())
}
