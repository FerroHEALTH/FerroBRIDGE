// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ValueSet/$expand`
//! (<https://hl7.org/fhir/R4/valueset-operation-expand.html>), whose answer
//! is the expanded `ValueSet` itself
//! (<https://hl7.org/fhir/R4/operations.html#response>).

use crate::support::{RELEASES, client, parameter, release_path, request_body};
use ferrobridge_term::config::WireVersion;
use ferrobridge_term::error::Error;
use ferrobridge_term::outcome::{ExpandOutcome, Expanded, Expansion};
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::stubs::terminology;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mounts `response` as the one answer of the `$expand` route of `version`.
async fn mount(server: &MockServer, version: WireVersion, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(format!("/{}/ValueSet/$expand", release_path(version))))
        .respond_with(response)
        .mount(server)
        .await;
}

/// Returns how many requests `server` received.
async fn received(server: &MockServer) -> Result<usize, Box<dyn std::error::Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("the mock server is not recording requests")?
        .len())
}

/// Returns the expansion `outcome` carries, or the error naming what it was.
fn expanded(outcome: ExpandOutcome) -> Result<Expanded, Box<dyn std::error::Error>> {
    match outcome {
        ExpandOutcome::Expanded(expanded) => Ok(expanded),
        other => Err(format!("expected an expansion, got {other:?}").into()),
    }
}

/// The member entry of the fixture value set.
fn member() -> serde_json::Value {
    terminology::expansion_contains(
        fixtures::TERM_SOURCE_SYSTEM,
        fixtures::TERM_MEMBER_CODE,
        fixtures::TERM_MEMBER_DISPLAY,
        Vec::new(),
    )
}

#[tokio::test]
async fn a_nested_expansion_keeps_its_hierarchy_in_both_releases() -> Result<(), Box<dyn std::error::Error>>
{
    for version in RELEASES {
        let server = MockServer::start().await;
        let mut child = member();
        child["version"] = serde_json::Value::from("1.0.0");
        child["inactive"] = serde_json::Value::Bool(true);
        let mut group = terminology::expansion_contains(
            fixtures::TERM_SOURCE_SYSTEM,
            "group",
            "Findings",
            vec![child],
        );
        group["abstract"] = serde_json::Value::Bool(true);
        mount(
            &server,
            version,
            terminology::expansion(fixtures::TERM_VALUE_SET_URL, Some(2), None, vec![group]),
        )
        .await;

        let answer = expanded(
            client(&server, version)?
                .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL))
                .await?,
        )?;
        assert_eq!(Some(fixtures::TERM_VALUE_SET_URL), answer.url.as_deref());
        assert_eq!(Some(2), answer.total);
        let parent = answer
            .contains
            .first()
            .ok_or("the parent entry was dropped")?;
        assert_eq!(Some("group"), parent.code.as_deref());
        assert_eq!(Some(true), parent.r#abstract);
        assert_eq!(1, parent.contains.len(), "the nesting was flattened");
        let nested = parent
            .contains
            .first()
            .ok_or("the nested entry was dropped")?;
        assert_eq!(Some(fixtures::TERM_SOURCE_SYSTEM), nested.system.as_deref());
        assert_eq!(Some(fixtures::TERM_MEMBER_CODE), nested.code.as_deref());
        assert_eq!(
            Some(fixtures::TERM_MEMBER_DISPLAY),
            nested.display.as_deref()
        );
        assert_eq!(Some("1.0.0"), nested.version.as_deref());
        assert_eq!(Some(true), nested.inactive);
        assert_eq!(None, nested.r#abstract);
    }
    Ok(())
}

#[tokio::test]
async fn the_request_carries_every_input_it_sets_under_its_declared_name()
-> Result<(), Box<dyn std::error::Error>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount(
            &server,
            version,
            terminology::expansion(fixtures::TERM_VALUE_SET_URL, None, None, Vec::new()),
        )
        .await;
        let expansion = Expansion::new(fixtures::TERM_VALUE_SET_URL)
            .with_version("2.0.0")
            .with_date("2026-01-01")
            .with_filter("alp")
            .with_page(20, 10)
            .with_designations(true);
        client(&server, version)?.expand(&expansion).await?;

        let body = request_body(&server, 0).await?;
        let expected = [
            (
                "url",
                "valueUri",
                serde_json::Value::from(fixtures::TERM_VALUE_SET_URL),
            ),
            (
                "valueSetVersion",
                "valueString",
                serde_json::Value::from("2.0.0"),
            ),
            (
                "date",
                "valueDateTime",
                serde_json::Value::from("2026-01-01"),
            ),
            ("filter", "valueString", serde_json::Value::from("alp")),
            ("offset", "valueInteger", serde_json::Value::from(20)),
            ("count", "valueInteger", serde_json::Value::from(10)),
            (
                "includeDesignations",
                "valueBoolean",
                serde_json::Value::Bool(true),
            ),
        ];
        for (name, member, value) in expected {
            assert_eq!(
                Some((member.to_owned(), value)),
                parameter(&body, name),
                "{name} in {}",
                version.as_str()
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn an_input_left_unset_is_not_sent() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount(
        &server,
        WireVersion::R4,
        terminology::expansion(fixtures::TERM_VALUE_SET_URL, None, None, Vec::new()),
    )
    .await;
    client(&server, WireVersion::R4)?
        .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL))
        .await?;

    let body = request_body(&server, 0).await?;
    for name in [
        "valueSetVersion",
        "date",
        "filter",
        "offset",
        "count",
        "includeDesignations",
    ] {
        assert_eq!(None, parameter(&body, name), "{name} was sent unset");
    }
    Ok(())
}

#[tokio::test]
async fn a_page_answers_its_offset_and_the_total_of_the_whole_expansion()
-> Result<(), Box<dyn std::error::Error>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount(
            &server,
            version,
            terminology::expansion(
                fixtures::TERM_VALUE_SET_URL,
                Some(45),
                Some(20),
                vec![member()],
            ),
        )
        .await;

        let answer = expanded(
            client(&server, version)?
                .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL).with_page(20, 1))
                .await?,
        )?;
        assert_eq!(Some(45), answer.total);
        assert_eq!(Some(20), answer.offset);
        assert_eq!(1, answer.contains.len());
    }
    Ok(())
}

#[tokio::test]
async fn a_count_of_zero_answers_the_size_alone() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount(
        &server,
        WireVersion::R4,
        terminology::expansion(fixtures::TERM_VALUE_SET_URL, Some(45), None, Vec::new()),
    )
    .await;

    let answer = expanded(
        client(&server, WireVersion::R4)?
            .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL).with_page(0, 0))
            .await?,
    )?;
    assert_eq!(Some(45), answer.total);
    assert_eq!(0, answer.contains.len());
    let body = request_body(&server, 0).await?;
    assert_eq!(
        Some(("valueInteger".to_owned(), serde_json::Value::from(0))),
        parameter(&body, "count")
    );
    Ok(())
}

#[tokio::test]
async fn the_filter_is_sent_and_the_filtered_codes_come_back() -> Result<(), Box<dyn std::error::Error>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount(
            &server,
            version,
            terminology::expansion(fixtures::TERM_VALUE_SET_URL, Some(1), None, vec![member()]),
        )
        .await;

        let answer = expanded(
            client(&server, version)?
                .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL).with_filter("alp"))
                .await?,
        )?;
        let body = request_body(&server, 0).await?;
        assert_eq!(
            Some(("valueString".to_owned(), serde_json::Value::from("alp"))),
            parameter(&body, "filter")
        );
        assert_eq!(
            vec![Some(fixtures::TERM_MEMBER_CODE)],
            answer
                .contains
                .iter()
                .map(|entry| entry.code.as_deref())
                .collect::<Vec<_>>()
        );
    }
    Ok(())
}

#[tokio::test]
async fn included_designations_are_read_from_each_entry() -> Result<(), Box<dyn std::error::Error>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        let mut entry = member();
        entry["designation"] = serde_json::json!([{
            "language": "nl",
            "use": {"system": "http://example.org/fhir/CodeSystem/designation-use", "code": "synonym"},
            "value": "Alfa bevinding",
        }]);
        mount(
            &server,
            version,
            terminology::expansion(fixtures::TERM_VALUE_SET_URL, None, None, vec![entry]),
        )
        .await;

        let answer = expanded(
            client(&server, version)?
                .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL).with_designations(true))
                .await?,
        )?;
        let entry = answer.contains.first().ok_or("the entry was dropped")?;
        let designation = entry
            .designations
            .first()
            .ok_or("the designation was dropped")?;
        assert_eq!(Some("nl"), designation.language.as_deref());
        assert_eq!("Alfa bevinding", designation.value);
        assert_eq!(
            Some("synonym"),
            designation
                .usage
                .as_ref()
                .and_then(|usage| usage.code.as_deref())
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_unknown_value_set_is_a_not_found_outcome_carrying_its_tx_issue()
-> Result<(), Box<dyn std::error::Error>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount(
            &server,
            version,
            terminology::outcome(
                404,
                "not-found",
                Some("not-found"),
                "the value set is not known",
            ),
        )
        .await;

        let outcome = client(&server, version)?
            .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL))
            .await?;
        let ExpandOutcome::NotFound { outcome } = outcome else {
            return Err(format!("expected a not-found outcome, got {outcome:?}").into());
        };
        assert_eq!(http::StatusCode::NOT_FOUND, outcome.status());
        assert!(
            outcome.has_tx_issue("not-found"),
            "the tx-issue-type coding was dropped"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_bare_404_is_a_not_found_outcome() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount(
        &server,
        WireVersion::R4,
        ResponseTemplate::new(404).set_body_string("not found"),
    )
    .await;

    let outcome = client(&server, WireVersion::R4)?
        .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL))
        .await?;
    let ExpandOutcome::NotFound { outcome } = outcome else {
        return Err(format!("expected a not-found outcome, got {outcome:?}").into());
    };
    assert_eq!("not found", outcome.body());
    Ok(())
}

#[tokio::test]
async fn an_expansion_error_is_a_refusal_carrying_the_outcome() -> Result<(), Box<dyn std::error::Error>> {
    for version in RELEASES {
        let server = MockServer::start().await;
        mount(
            &server,
            version,
            terminology::outcome(
                422,
                "too-costly",
                Some("vs-invalid"),
                "the expansion is too large",
            ),
        )
        .await;

        let error = client(&server, version)?
            .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL))
            .await
            .expect_err("an expansion error is a refusal, never an empty expansion");
        let Error::Refused {
            operation,
            upstream,
            ..
        } = &error
        else {
            return Err(format!("expected a refusal, got {error:?}").into());
        };
        assert_eq!(ferrobridge_term::EXPAND, *operation);
        assert_eq!(http::StatusCode::UNPROCESSABLE_ENTITY, upstream.status());
        assert_eq!(
            Some("too-costly"),
            upstream
                .outcome()
                .and_then(|outcome| outcome.issues.first())
                .map(|issue| issue.code.as_str())
        );
        assert!(upstream.has_tx_issue("vs-invalid"));
        assert!(!error.is_retryable());
        assert_eq!(1, received(&server).await?, "a refusal was retried");
    }
    Ok(())
}

#[tokio::test]
async fn an_invalid_code_issue_is_a_refusal_and_not_an_unknown_value_set()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount(
        &server,
        WireVersion::R4,
        terminology::outcome(400, "code-invalid", Some("invalid-code"), "a bad code"),
    )
    .await;

    let error = client(&server, WireVersion::R4)?
        .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL))
        .await
        .expect_err("invalid-code names a code, not a value set");
    assert_eq!("refused", error.kind());
    Ok(())
}

#[tokio::test]
async fn a_parameters_answer_is_a_decode_error() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let wrapped = serde_json::json!({
        "resourceType": "Parameters",
        "parameter": [{
            "name": "return",
            "resource": terminology::expansion_body(
                fixtures::TERM_VALUE_SET_URL, None, None, Vec::new()
            ),
        }],
    });
    mount(
        &server,
        WireVersion::R4,
        ResponseTemplate::new(200).set_body_string(wrapped.to_string()),
    )
    .await;

    let error = client(&server, WireVersion::R4)?
        .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL))
        .await
        .expect_err("the answer is the ValueSet itself, not a Parameters");
    assert_eq!("body", error.kind());
    Ok(())
}

#[tokio::test]
async fn a_value_set_with_no_expansion_is_a_decode_error() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let bare = serde_json::json!({
        "resourceType": "ValueSet",
        "url": fixtures::TERM_VALUE_SET_URL,
        "status": "active",
    });
    mount(
        &server,
        WireVersion::R4,
        ResponseTemplate::new(200).set_body_string(bare.to_string()),
    )
    .await;

    let error = client(&server, WireVersion::R4)?
        .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL))
        .await
        .expect_err("a ValueSet without an expansion is not an empty expansion");
    assert_eq!("body", error.kind());
    Ok(())
}

#[tokio::test]
async fn a_page_beyond_a_fhir_integer_is_refused_and_nothing_is_sent()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let error = client(&server, WireVersion::R4)?
        .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL).with_page(u32::MAX, 10))
        .await
        .expect_err("an offset past i32::MAX has no FHIR integer form");
    assert!(
        matches!(
            error,
            Error::ParameterRange {
                parameter: "offset",
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(0, received(&server).await?);
    Ok(())
}

#[tokio::test]
async fn an_stu3_expansion_sends_its_inputs_and_reads_the_value_set()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount(
        &server,
        WireVersion::Stu3,
        terminology::expansion(fixtures::TERM_VALUE_SET_URL, Some(1), None, vec![member()]),
    )
    .await;

    let answer = expanded(
        client(&server, WireVersion::Stu3)?
            .expand(
                &Expansion::new(fixtures::TERM_VALUE_SET_URL)
                    .with_date("2026-01-01")
                    .with_page(0, 5),
            )
            .await?,
    )?;
    assert_eq!(1, answer.contains.len());
    let body = request_body(&server, 0).await?;
    assert_eq!(
        Some((
            "valueDateTime".to_owned(),
            serde_json::Value::from("2026-01-01")
        )),
        parameter(&body, "date")
    );
    assert_eq!(
        Some(("valueInteger".to_owned(), serde_json::Value::from(5))),
        parameter(&body, "count")
    );
    Ok(())
}

#[tokio::test]
async fn a_value_set_version_is_refused_in_stu3_and_nothing_is_sent()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let error = client(&server, WireVersion::Stu3)?
        .expand(&Expansion::new(fixtures::TERM_VALUE_SET_URL).with_version("2.0.0"))
        .await
        .expect_err("STU3 $expand declares no valueSetVersion");
    assert!(
        matches!(
            error,
            Error::UnsupportedParameter {
                operation: ferrobridge_term::EXPAND,
                parameter: "valueSetVersion",
                wire_version: WireVersion::Stu3,
            }
        ),
        "{error:?}"
    );
    assert_eq!(0, received(&server).await?);
    Ok(())
}
