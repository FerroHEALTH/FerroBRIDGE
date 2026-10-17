// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ConceptMap/$translate`
//! (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>).

use crate::support::{RELEASES, client, parameter, release_path, request_body};
use ferrobridge_term::concept::Equivalence;
use ferrobridge_term::config::WireVersion;
use ferrobridge_term::error::Error;
use ferrobridge_term::outcome::{Request, TranslateOutcome, Translation};
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::stubs::terminology;
use std::error::Error as StdError;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The route the operation is invoked on, under the release prefix.
fn route(version: WireVersion) -> String {
    format!("/{}/ConceptMap/$translate", release_path(version))
}

/// Mounts `response` as the one answer of the `$translate` route.
async fn mount(server: &MockServer, version: WireVersion, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(route(version)))
        .respond_with(response)
        .mount(server)
        .await;
}

/// Sends the fixture translation of `code` to `server`.
async fn translate(
    server: &MockServer,
    version: WireVersion,
    code: &str,
) -> Result<TranslateOutcome, Error> {
    client(server, version)
        .expect("a buildable client")
        .translate(
            &Translation::new(fixtures::TERM_SOURCE_SYSTEM, code)
                .with_concept_map(fixtures::TERM_CONCEPT_MAP_URL)
                .with_target_system(fixtures::TERM_TARGET_SYSTEM),
        )
        .await
}

#[tokio::test]
async fn an_equivalent_match_is_a_translation_in_both_releases() -> Result<(), Box<dyn StdError>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount(
            &server,
            version,
            terminology::translate(
                "equivalent",
                fixtures::TERM_TARGET_SYSTEM,
                fixtures::TERM_MEMBER_TARGET_CODE,
                "Target alpha",
            ),
        )
        .await;

        let outcome = translate(&server, version, fixtures::TERM_MEMBER_CODE).await?;
        let TranslateOutcome::Translated { matches, .. } = &outcome else {
            return Err(format!("expected a translation, got {outcome:?}").into());
        };
        assert_eq!(1, matches.len());
        let accepted: Vec<Option<&str>> = outcome
            .accepted()
            .map(|found| {
                found
                    .concept
                    .as_ref()
                    .and_then(|concept| concept.code.as_deref())
            })
            .collect();
        assert_eq!(vec![Some(fixtures::TERM_MEMBER_TARGET_CODE)], accepted);
    }
    Ok(())
}

#[tokio::test]
async fn a_wider_or_inexact_match_is_returned_and_never_accepted() -> Result<(), Box<dyn StdError>>
{
    let server = MockServer::start().await;
    mount(
        &server,
        WireVersion::R4,
        terminology::translate_matches(&[
            (
                "wider",
                fixtures::TERM_TARGET_SYSTEM,
                "B1",
                "Target beta group",
            ),
            (
                "inexact",
                fixtures::TERM_TARGET_SYSTEM,
                "B2",
                "Target beta neighbour",
            ),
        ]),
    )
    .await;

    let outcome = translate(&server, WireVersion::R4, fixtures::TERM_NON_MEMBER_CODE).await?;
    let TranslateOutcome::Translated { matches, .. } = &outcome else {
        return Err(format!("expected the matches, got {outcome:?}").into());
    };
    assert_eq!(
        2,
        matches.len(),
        "the caller checks each match, so none is dropped"
    );
    assert_eq!(
        vec![Some("wider"), Some("inexact")],
        matches
            .iter()
            .map(|found| found.equivalence.as_ref().map(Equivalence::as_str))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        0,
        outcome.accepted().count(),
        "a wider or inexact match is not a translation"
    );
    Ok(())
}

#[tokio::test]
async fn a_false_result_is_a_no_match_outcome() -> Result<(), Box<dyn StdError>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount(
            &server,
            version,
            terminology::translate_no_match("no map for this code"),
        )
        .await;

        let outcome = translate(&server, version, fixtures::TERM_UNKNOWN_CODE).await?;
        let TranslateOutcome::NoMatch { message } = &outcome else {
            return Err(format!("expected no match, got {outcome:?}").into());
        };
        assert_eq!(Some("no map for this code"), message.as_deref());
        assert_eq!(0, outcome.accepted().count());
    }
    Ok(())
}

#[tokio::test]
async fn the_request_carries_the_declared_parameter_names() -> Result<(), Box<dyn StdError>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount(
            &server,
            version,
            terminology::translate(
                "equivalent",
                fixtures::TERM_TARGET_SYSTEM,
                fixtures::TERM_MEMBER_TARGET_CODE,
                "Target alpha",
            ),
        )
        .await;
        translate(&server, version, fixtures::TERM_MEMBER_CODE).await?;

        let body = request_body(&server, 0).await?;
        assert_eq!(
            Some((
                "valueUri".to_owned(),
                serde_json::Value::from(fixtures::TERM_SOURCE_SYSTEM)
            )),
            parameter(&body, "system")
        );
        assert_eq!(
            Some((
                "valueCode".to_owned(),
                serde_json::Value::from(fixtures::TERM_MEMBER_CODE)
            )),
            parameter(&body, "code")
        );
        assert_eq!(
            Some((
                "valueUri".to_owned(),
                serde_json::Value::from(fixtures::TERM_TARGET_SYSTEM)
            )),
            parameter(&body, "targetsystem"),
            "both releases spell the target system parameter `targetsystem`"
        );
        assert_eq!(
            Some((
                "valueUri".to_owned(),
                serde_json::Value::from(fixtures::TERM_CONCEPT_MAP_URL)
            )),
            parameter(&body, "url")
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_unknown_concept_map_is_a_typed_error_and_never_a_missing_translation()
-> Result<(), Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount(
        &server,
        WireVersion::R4,
        terminology::outcome(
            404,
            "not-found",
            Some("not-found"),
            "no ConceptMap with that canonical",
        ),
    )
    .await;

    let error = translate(&server, WireVersion::R4, fixtures::TERM_MEMBER_CODE)
        .await
        .expect_err("an unknown concept map fails closed");
    let Error::Refused {
        operation,
        upstream,
        ..
    } = &error
    else {
        return Err(format!("expected a refusal, got {error:?}").into());
    };
    assert_eq!(ferrobridge_term::TRANSLATE, *operation);
    assert!(upstream.has_tx_issue("not-found"));
    Ok(())
}

/// Sends `translation` to a server that answers `equivalent` and returns
/// the request body the server received.
async fn sent(
    version: WireVersion,
    translation: &Translation,
) -> Result<serde_json::Value, Box<dyn StdError>> {
    let server = MockServer::start().await;
    mount(
        &server,
        version,
        terminology::translate(
            "equivalent",
            fixtures::TERM_TARGET_SYSTEM,
            fixtures::TERM_MEMBER_TARGET_CODE,
            "Target alpha",
        ),
    )
    .await;
    client(&server, version)?.translate(translation).await?;
    request_body(&server, 0).await
}

#[tokio::test]
async fn a_map_url_alone_sends_no_target_system_and_no_reverse() -> Result<(), Box<dyn StdError>> {
    for version in RELEASES {
        let translation =
            Translation::new(fixtures::TERM_SOURCE_SYSTEM, fixtures::TERM_MEMBER_CODE)
                .with_concept_map(fixtures::TERM_CONCEPT_MAP_URL);
        let body = sent(version, &translation).await?;
        assert_eq!(
            Some((
                "valueUri".to_owned(),
                serde_json::Value::from(fixtures::TERM_CONCEPT_MAP_URL)
            )),
            parameter(&body, "url")
        );
        assert_eq!(
            None,
            parameter(&body, "targetsystem"),
            "targetsystem is 0..1 and was not given"
        );
        assert_eq!(None, parameter(&body, "reverse"), "reverse was not given");
        assert_eq!(None, parameter(&body, "version"), "version was not given");
    }
    Ok(())
}

#[tokio::test]
async fn the_system_version_is_sent_when_given() -> Result<(), Box<dyn StdError>> {
    for version in RELEASES {
        let translation =
            Translation::new(fixtures::TERM_SOURCE_SYSTEM, fixtures::TERM_MEMBER_CODE)
                .with_version("2026")
                .with_concept_map(fixtures::TERM_CONCEPT_MAP_URL);
        let body = sent(version, &translation).await?;
        assert_eq!(
            Some(("valueString".to_owned(), serde_json::Value::from("2026"))),
            parameter(&body, "version")
        );
    }
    Ok(())
}

#[tokio::test]
async fn reverse_is_sent_as_a_boolean_only_when_given() -> Result<(), Box<dyn StdError>> {
    for version in RELEASES {
        for reverse in [true, false] {
            let translation = Translation::new(
                fixtures::TERM_TARGET_SYSTEM,
                fixtures::TERM_MEMBER_TARGET_CODE,
            )
            .with_concept_map(fixtures::TERM_CONCEPT_MAP_URL)
            .with_reverse(reverse);
            let body = sent(version, &translation).await?;
            assert_eq!(
                Some(("valueBoolean".to_owned(), serde_json::Value::from(reverse))),
                parameter(&body, "reverse")
            );
            assert_eq!(None, parameter(&body, "targetsystem"));
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_batch_translation_carries_the_same_optional_parameters() -> Result<(), Box<dyn StdError>>
{
    let server = MockServer::start().await;
    let no_match = terminology::parameters_resource(&[serde_json::json!({
        "name": "result",
        "valueBoolean": false,
    })]);
    Mock::given(method("POST"))
        .and(path(format!("/{}", release_path(WireVersion::R4))))
        .respond_with(terminology::batch_response(&[
            ("200 OK", no_match.clone()),
            ("200 OK", no_match),
        ]))
        .mount(&server)
        .await;

    let requests = vec![
        Request::Translate(
            Translation::new(fixtures::TERM_SOURCE_SYSTEM, fixtures::TERM_MEMBER_CODE)
                .with_concept_map(fixtures::TERM_CONCEPT_MAP_URL),
        ),
        Request::Translate(
            Translation::new(
                fixtures::TERM_TARGET_SYSTEM,
                fixtures::TERM_MEMBER_TARGET_CODE,
            )
            .with_concept_map(fixtures::TERM_CONCEPT_MAP_URL)
            .with_reverse(true),
        ),
    ];
    let answers = client(&server, WireVersion::R4)?.batch(&requests).await?;
    assert_eq!(2, answers.len());

    let body = request_body(&server, 0).await?;
    let forward = &body["entry"][0]["resource"];
    let backward = &body["entry"][1]["resource"];
    assert_eq!(None, parameter(forward, "reverse"));
    assert_eq!(None, parameter(forward, "targetsystem"));
    assert_eq!(
        Some(("valueBoolean".to_owned(), serde_json::Value::from(true))),
        parameter(backward, "reverse")
    );
    assert_eq!(
        Some((
            "valueUri".to_owned(),
            serde_json::Value::from(fixtures::TERM_CONCEPT_MAP_URL)
        )),
        parameter(backward, "url")
    );
    Ok(())
}
