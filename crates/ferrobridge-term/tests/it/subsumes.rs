// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `CodeSystem/$subsumes`
//! (<https://hl7.org/fhir/R4/codesystem-operation-subsumes.html>), whose
//! `outcome` is bound `required` to `ConceptSubsumptionOutcome`.

use crate::support::{RELEASES, client, parameter, release_path, request_body};
use ferrobridge_term::config::WireVersion;
use ferrobridge_term::error::Error;
use ferrobridge_term::outcome::{SubsumesOutcome, Subsumption};
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::stubs::terminology;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mounts `response` as the one answer of the `$subsumes` route of `version`.
async fn mount(server: &MockServer, version: WireVersion, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(format!(
            "/{}/CodeSystem/$subsumes",
            release_path(version)
        )))
        .respond_with(response)
        .mount(server)
        .await;
}

/// The question every case asks: how the member code relates to the
/// non-member code.
fn question() -> Subsumption {
    Subsumption::new(
        fixtures::TERM_SOURCE_SYSTEM,
        fixtures::TERM_MEMBER_CODE,
        fixtures::TERM_NON_MEMBER_CODE,
    )
}

#[tokio::test]
async fn each_of_the_four_outcomes_is_read_in_every_release()
-> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        ("equivalent", SubsumesOutcome::Equivalent),
        ("subsumes", SubsumesOutcome::Subsumes),
        ("subsumed-by", SubsumesOutcome::SubsumedBy),
        ("not-subsumed", SubsumesOutcome::NotSubsumed),
    ];
    for version in [WireVersion::Stu3, WireVersion::R4, WireVersion::R4B] {
        for (code, expected) in cases {
            let server = MockServer::start().await;
            mount(&server, version, terminology::subsumes(code)).await;
            let outcome = client(&server, version)?.subsumes(&question()).await?;
            assert_eq!(expected, outcome, "{code} in {}", version.as_str());
        }
    }
    Ok(())
}

#[tokio::test]
async fn the_request_carries_the_declared_parameter_names() -> Result<(), Box<dyn std::error::Error>>
{
    for version in RELEASES {
        let server = MockServer::start().await;
        mount(&server, version, terminology::subsumes("subsumes")).await;
        client(&server, version)?
            .subsumes(&question().with_version("1.0.0"))
            .await?;

        let body = request_body(&server, 0).await?;
        let expected = [
            ("system", "valueUri", fixtures::TERM_SOURCE_SYSTEM),
            ("codeA", "valueCode", fixtures::TERM_MEMBER_CODE),
            ("codeB", "valueCode", fixtures::TERM_NON_MEMBER_CODE),
            ("version", "valueString", "1.0.0"),
        ];
        for (name, member, value) in expected {
            assert_eq!(
                Some((member.to_owned(), serde_json::Value::from(value))),
                parameter(&body, name),
                "{name} in {}",
                version.as_str()
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_version_left_unset_is_not_sent() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount(
        &server,
        WireVersion::R4,
        terminology::subsumes("not-subsumed"),
    )
    .await;
    client(&server, WireVersion::R4)?
        .subsumes(&question())
        .await?;
    assert_eq!(None, parameter(&request_body(&server, 0).await?, "version"));
    Ok(())
}

#[tokio::test]
async fn an_outcome_outside_the_value_set_is_a_decode_error()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount(&server, WireVersion::R4, terminology::subsumes("related")).await;

    let error = client(&server, WireVersion::R4)?
        .subsumes(&question())
        .await
        .expect_err("the outcome binding is required");
    assert_eq!("body", error.kind());
    Ok(())
}

#[tokio::test]
async fn an_undeterminable_relationship_is_a_refusal_carrying_its_tx_issue()
-> Result<(), Box<dyn std::error::Error>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount(
            &server,
            version,
            terminology::outcome(
                400,
                "code-invalid",
                Some("invalid-code"),
                "the code is not in the code system",
            ),
        )
        .await;

        let error = client(&server, version)?
            .subsumes(&question())
            .await
            .expect_err("an unknown code is an error response");
        let Error::Refused {
            operation,
            upstream,
            ..
        } = &error
        else {
            return Err(format!("expected a refusal, got {error:?}").into());
        };
        assert_eq!(ferrobridge_term::SUBSUMES, *operation);
        assert_eq!(http::StatusCode::BAD_REQUEST, upstream.status());
        assert!(upstream.has_tx_issue("invalid-code"));
    }
    Ok(())
}
