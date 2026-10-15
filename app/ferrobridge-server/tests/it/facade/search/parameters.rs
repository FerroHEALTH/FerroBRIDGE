// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Each parameter type, `_id`, `_sort`, and the strict and lenient handling
//! of what the type does not declare.

use axum::body::Body;
use http::Request;
use http::StatusCode;
use std::error::Error as StdError;

use super::super::CONTAINER;
use super::super::VERSION_ONE;
use super::super::call;
use super::super::create_one;
use super::super::first_issue;
use super::bound;
use super::get;
use super::link;
use super::matches;
use super::mount_rows;
use super::queries;
use super::row;
use super::searching;

/// The path the fixture declares for the code of `code`.
const CODE: &str = "c/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0002]/value/defining_code/code_string";

/// The path the fixture declares for the system of `code`.
const SYSTEM: &str = "c/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0002]/value/defining_code/terminology_id/value";

/// The path the fixture declares for `onset-date`.
const ONSET: &str =
    "c/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0077]/value/value";

/// The path the fixture declares for `diagnosis-name`.
const NAME: &str =
    "c/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0002]/value/value";

/// Runs `query` against an empty CDR and returns the one row query it sent.
async fn query_of(query: &str) -> Result<serde_json::Value, Box<dyn StdError>> {
    let harness = searching().await;
    mount_rows(&harness.cdr, &[]).await;
    let (status, body) = get(&harness, &format!("Condition?{query}")).await?;
    assert_eq!(StatusCode::OK, status, "{query}: {body}");
    let sent = queries(&harness).await;
    assert_eq!(1, sent.len(), "{query}");
    Ok(sent.into_iter().next().unwrap_or_default())
}

#[tokio::test]
async fn a_token_with_a_system_compares_the_translated_system_and_the_code()
-> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.4.10: [system]|[code] matches both.
    let query = query_of("code=http://snomed.info/sct%7C44054006").await?;
    let text = query["q"].as_str().unwrap_or_default();
    assert!(text.contains(&format!("{SYSTEM} = $")), "{text}");
    assert!(text.contains(&format!("{CODE} = $")), "{text}");
    let values = bound(&query);
    assert!(values.contains(&String::from("SNOMED-CT")), "{values:?}");
    assert!(values.contains(&String::from("44054006")), "{values:?}");
    assert!(
        !values.contains(&String::from("http://snomed.info/sct")),
        "{values:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_token_with_an_empty_system_asks_for_no_system() -> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.4.10: |[code] matches a code with no system.
    let query = query_of("code=%7C44054006").await?;
    let text = query["q"].as_str().unwrap_or_default();
    assert!(text.contains(&format!("NOT EXISTS {SYSTEM}")), "{text}");
    Ok(())
}

#[tokio::test]
async fn comma_values_are_alternatives_and_repeats_are_all_required()
-> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.4.17: a comma is OR, a repeated parameter AND.
    let query = query_of("code=a,b&code=c").await?;
    let text = query["q"].as_str().unwrap_or_default();
    assert!(text.contains(") OR ("), "{text}");
    assert_eq!(3, text.matches(&format!("{CODE} = $")).count(), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_date_compares_the_range_its_precision_spans() -> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.4.7: 2026-01 is the whole month.
    let query = query_of("onset-date=2026-01").await?;
    let text = query["q"].as_str().unwrap_or_default();
    assert!(text.contains(&format!("{ONSET} >= $")), "{text}");
    assert!(text.contains(&format!("{ONSET} < $")), "{text}");
    let values = bound(&query);
    assert!(values.contains(&String::from("2026-01")), "{values:?}");
    assert!(values.contains(&String::from("2026-02")), "{values:?}");
    let query = query_of("onset-date=ge2026-09-15T10:00Z").await?;
    let values = bound(&query);
    assert!(
        values.contains(&String::from("2026-09-15T10:00Z")),
        "{values:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_string_starts_contains_or_matches_exactly() -> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.4.8.
    let start = query_of("diagnosis-name=Synth").await?;
    assert!(bound(&start).contains(&String::from("Synth*")));
    assert!(
        start["q"]
            .as_str()
            .unwrap_or_default()
            .contains(&format!("{NAME} LIKE $"))
    );
    let contains = query_of("diagnosis-name:contains=problem").await?;
    assert!(bound(&contains).contains(&String::from("*problem*")));
    let exact = query_of("diagnosis-name:exact=Synthetic%20problem%20one").await?;
    assert!(
        exact["q"]
            .as_str()
            .unwrap_or_default()
            .contains(&format!("{NAME} = $"))
    );
    assert!(bound(&exact).contains(&String::from("Synthetic problem one")));
    Ok(())
}

#[tokio::test]
async fn sort_orders_by_the_declared_paths_before_the_total_order() -> Result<(), Box<dyn StdError>>
{
    // R4 search.html §3.1.1.5.1: `-` orders a key downward.
    let query = query_of("_sort=-onset-date,code").await?;
    let text = query["q"].as_str().unwrap_or_default();
    assert!(
        text.contains(&format!(
            "ORDER BY {ONSET} DESC, {CODE} ASC, v/commit_audit/time_committed/value ASC, v/uid/value ASC"
        )),
        "{text}"
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_parameter_is_refused_unless_the_client_asks_for_lenient()
-> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.3: Prefer: handling=strict|lenient.
    let harness = searching().await;
    let (status, body) = get(
        &harness,
        "Condition?patient=p-1&verification-status=confirmed",
    )
    .await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    let issue = first_issue(&body);
    assert_eq!(Some("not-supported"), issue["code"].as_str());
    assert!(
        issue["diagnostics"]
            .as_str()
            .is_some_and(|text| text.contains("verification-status")),
        "{body}"
    );
    assert!(
        queries(&harness).await.is_empty(),
        "a refused search reached the CDR"
    );

    mount_rows(&harness.cdr, &[row(VERSION_ONE, &harness.composition())]).await;
    let (status, body) = call(
        harness.app(),
        Request::get("/fhir/Condition?patient=p-1&verification-status=confirmed")
            .header("Prefer", "handling=lenient")
            .body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(1, matches(&body).len());
    assert_eq!(
        Some("http://ferrobridge.invalid/fhir/Condition?patient=p-1&_count=20"),
        link(&body, "self"),
        "the self link names only the parameters the search used"
    );
    let outcome = body["entry"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["search"]["mode"].as_str() == Some("outcome"))
        })
        .expect("an outcome entry reports the ignored parameter");
    assert_eq!(
        Some("warning"),
        outcome["resource"]["issue"][0]["severity"].as_str()
    );
    Ok(())
}

#[tokio::test]
async fn an_unsupported_modifier_is_refused_even_under_lenient_handling()
-> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.4.4: "Server SHALL reject any search request that
    // contains ... a modifier that the server does not support".
    let harness = searching().await;
    for query in [
        "code:text=diabetes",
        "patient:identifier=http://example.org/mrn%7C1",
        "onset-date:missing=true",
        "_id:not=x",
    ] {
        let (status, body) = call(
            harness.app(),
            Request::get(format!("/fhir/Condition?{query}"))
                .header("Prefer", "handling=lenient")
                .body(Body::empty())?,
        )
        .await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {body}");
        assert_eq!(
            Some("not-supported"),
            first_issue(&body)["code"].as_str(),
            "{query}"
        );
    }
    assert!(queries(&harness).await.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_malformed_value_is_invalid_and_the_ap_prefix_unsupported()
-> Result<(), Box<dyn StdError>> {
    let harness = searching().await;
    for (query, code) in [
        ("onset-date=yesterday", "invalid"),
        ("onset-date=ap2026", "not-supported"),
        ("code=a%7Cb%7Cc", "invalid"),
        ("diagnosis-name=a*b", "not-supported"),
        ("_count=many", "invalid"),
        ("_total=some", "invalid"),
    ] {
        let (status, body) = get(&harness, &format!("Condition?{query}")).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {body}");
        assert_eq!(
            Some(code),
            first_issue(&body)["code"].as_str(),
            "{query}: {body}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_empty_parameter_is_ignored_and_left_out_of_the_self_link()
-> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.3: "Empty parameters are not an error".
    let harness = searching().await;
    mount_rows(&harness.cdr, &[]).await;
    let (status, body) = get(&harness, "Condition?code=&patient=p-1").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(
        Some("http://ferrobridge.invalid/fhir/Condition?patient=p-1&_count=20"),
        link(&body, "self")
    );
    Ok(())
}

#[tokio::test]
async fn id_resolves_through_the_identity_map_to_the_composition() -> Result<(), Box<dyn StdError>>
{
    let harness = searching().await;
    let id = create_one(&harness).await?;
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &harness.composition())]).await;
    let (status, body) = get(&harness, &format!("Condition?_id={id}")).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let found = matches(&body);
    assert_eq!(1, found.len(), "{body}");
    assert_eq!(
        Some(id.as_str()),
        found
            .first()
            .and_then(|entry| entry["resource"]["id"].as_str())
    );
    let sent = queries(&harness).await;
    let query = sent.first().expect("the _id search reached the CDR");
    assert!(
        query["q"]
            .as_str()
            .unwrap_or_default()
            .contains("v/uid/value LIKE $"),
        "{query}"
    );
    assert!(bound(query).contains(&format!("{CONTAINER}::*")), "{query}");

    let (status, body) = get(&harness, "Condition?_id=no-such-id").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some(0), body["total"].as_u64());
    assert_eq!(
        1,
        queries(&harness).await.len(),
        "an unknown id reached the CDR"
    );
    Ok(())
}
