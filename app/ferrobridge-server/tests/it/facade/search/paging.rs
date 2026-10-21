// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Paging: the page size and its cap, the count forms, the `next` link
//! round trip over the CDR's `offset` and `fetch`, and `Bundle.total`.

use ferrobridge_server::facade::search::Limits;
use http::StatusCode;
use std::error::Error as StdError;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::super::VERSION_ONE;
use super::super::first_issue;
use super::AQL;
use super::OTHER_CONTAINER;
use super::counts;
use super::get;
use super::link;
use super::matches;
use super::mount_count;
use super::mount_rows;
use super::queries;
use super::result_set;
use super::row;
use super::searching;
use super::searching_with;

/// Returns the path and query of an absolute link under the test base.
fn relative(url: &str) -> &str {
    url.strip_prefix("http://ferrobridge.invalid/fhir/")
        .unwrap_or(url)
}

#[tokio::test]
async fn a_count_above_the_cap_is_answered_with_the_cap() -> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.5.3: "Servers SHALL NOT return more resources than
    // requested ... but may return less than the client requested".
    let harness = searching_with(Limits {
        default_count: 1,
        max_count: 2,
    })
    .await;
    mount_rows(&harness.cdr, &[]).await;
    let (status, body) = get(&harness, "Condition?_count=500").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(
        Some("http://ferrobridge.invalid/fhir/Condition?_count=2"),
        link(&body, "self"),
        "the self link names the page size used"
    );
    let sent = queries(&harness).await;
    assert_eq!(
        Some(3),
        sent.first().and_then(|query| query["fetch"].as_i64())
    );
    let (_, body) = get(&harness, "Condition").await?;
    assert_eq!(
        Some("http://ferrobridge.invalid/fhir/Condition?_count=1"),
        link(&body, "self"),
        "a search with no _count pages at the configured default"
    );
    Ok(())
}

#[tokio::test]
async fn a_percent_encoded_count_reads_as_its_decoded_value() -> Result<(), Box<dyn StdError>> {
    // Search parameters are application/x-www-form-urlencoded
    // (<https://hl7.org/fhir/R4/http.html#search>), so `%32` is `2`: the
    // request guard and the search read the one decoded value.
    let harness = searching().await;
    mount_rows(&harness.cdr, &[]).await;
    let (status, body) = get(&harness, "Condition?_count=%32").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(
        Some("http://ferrobridge.invalid/fhir/Condition?_count=2"),
        link(&body, "self"),
        "{body}"
    );
    assert_eq!(
        Some(3),
        queries(&harness)
            .await
            .first()
            .and_then(|query| query["fetch"].as_i64())
    );
    let (status, body) = get(&harness, "Condition?_count=m%61ny").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    assert_eq!(Some("invalid"), first_issue(&body)["code"].as_str());
    Ok(())
}

#[tokio::test]
async fn count_zero_answers_the_total_with_no_entries_and_no_next_link()
-> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.5.3: "if _count has the value 0, this shall be
    // treated the same as _summary=count: the server returns a bundle that
    // reports the total number of resources that match in Bundle.total, but
    // with no entries, and no prev/next/last links".
    // The total counts entries (#658) with one count query that reads no
    // composition (#661).
    for query in [
        "Condition?patient=p-1&_count=0",
        "Condition?patient=p-1&_summary=count",
    ] {
        let harness = searching().await;
        let composition = harness.composition();
        let second = format!("{OTHER_CONTAINER}::ferrobridge.test::1");
        mount_rows(
            &harness.cdr,
            &[row(VERSION_ONE, &composition), row(&second, &composition)],
        )
        .await;
        mount_count(&harness.cdr, &[composition.clone(), composition]).await;
        let (status, body) = get(&harness, query).await?;
        assert_eq!(StatusCode::OK, status, "{query}: {body}");
        assert_eq!(Some(2), body["total"].as_u64(), "{query}: {body}");
        assert!(matches(&body).is_empty(), "{query}: {body}");
        assert_eq!(None, link(&body, "next"), "{query}");
        let sent = queries(&harness).await;
        assert_eq!(1, sent.len(), "{query}: one count query: {sent:?}");
        assert_eq!(1, counts(&sent).len(), "{query}: no row read: {sent:?}");
        assert_eq!(
            None,
            sent.first().and_then(|query| query["fetch"].as_i64()),
            "{query}: a count is no page"
        );
    }
    Ok(())
}

#[tokio::test]
async fn count_zero_for_an_unknown_subject_needs_no_cdr_call() -> Result<(), Box<dyn StdError>> {
    let harness = searching().await;
    let (status, body) = get(&harness, "Condition?patient=nobody&_count=0").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some(0), body["total"].as_u64());
    assert_eq!(queries(&harness).await, Vec::<serde_json::Value>::new());
    Ok(())
}

#[tokio::test]
async fn the_next_link_round_trips_across_a_page_boundary_in_a_stable_order()
-> Result<(), Box<dyn StdError>> {
    let harness = searching().await;
    let composition = harness.composition();
    let second = format!("{OTHER_CONTAINER}::ferrobridge.test::1");
    Mock::given(matchers::method("POST"))
        .and(matchers::path(AQL))
        .and(matchers::body_partial_json(
            serde_json::json!({ "offset": 0, "fetch": 2 }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(result_set(&[
            row(VERSION_ONE, &composition),
            row(&second, &composition),
        ])))
        .mount(&harness.cdr)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path(AQL))
        .and(matchers::body_partial_json(
            serde_json::json!({ "offset": 1, "fetch": 2 }),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(result_set(&[row(&second, &composition)])),
        )
        .mount(&harness.cdr)
        .await;
    let (status, first) = get(&harness, "Condition?patient=p-1&_count=1").await?;
    assert_eq!(StatusCode::OK, status, "{first}");
    assert_eq!(1, matches(&first).len());
    assert_eq!(
        None,
        first["total"].as_u64(),
        "a page with a next page has no cheap total"
    );
    let next = link(&first, "next").expect("a row is left, so a next link is written");
    assert!(next.contains("_count=1"), "{next}");
    assert!(next.contains("_cursor="), "{next}");
    let (status, second_page) = get(&harness, relative(next)).await?;
    assert_eq!(StatusCode::OK, status, "{second_page}");
    assert_eq!(1, matches(&second_page).len());
    assert_eq!(None, link(&second_page, "next"));
    assert_eq!(Some(next), link(&second_page, "self"));
    let ids: Vec<&str> = [&first, &second_page]
        .iter()
        .filter_map(|page| {
            matches(page)
                .first()
                .and_then(|entry| entry["resource"]["id"].as_str())
        })
        .collect();
    assert_eq!(2, ids.len());
    assert_ne!(ids.first(), ids.get(1), "the second page repeats the first");
    let orders: Vec<String> = queries(&harness)
        .await
        .iter()
        .filter_map(|query| query["q"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(2, orders.len());
    assert_eq!(
        orders.first(),
        orders.get(1),
        "both pages read one query in one order"
    );
    Ok(())
}

#[tokio::test]
async fn total_accurate_counts_the_rows_of_a_paged_search() -> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.5.2: _total=accurate asks for the exact number.
    let harness = searching().await;
    let composition = harness.composition();
    let second = format!("{OTHER_CONTAINER}::ferrobridge.test::1");
    mount_rows(
        &harness.cdr,
        &[row(VERSION_ONE, &composition), row(&second, &composition)],
    )
    .await;
    mount_count(&harness.cdr, &[composition.clone(), composition]).await;
    let (status, body) = get(&harness, "Condition?patient=p-1&_count=1&_total=accurate").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(
        Some(2),
        body["total"].as_u64(),
        "the total counts matches, never the page"
    );
    assert_eq!(1, matches(&body).len());
    let (_, body) = get(&harness, "Condition?patient=p-1&_total=none").await?;
    assert_eq!(
        None,
        body["total"].as_u64(),
        "_total=none asks for no total"
    );
    // Only the accurate search counted, with one count query (#661), and no
    // query read the rows at the cap (100) to count them.
    let sent = queries(&harness).await;
    assert_eq!(
        1,
        counts(&sent).len(),
        "only the accurate search counted: {sent:?}"
    );
    assert!(
        sent.iter()
            .all(|query| query["fetch"].as_i64() != Some(100)),
        "no count read the rows: {sent:?}"
    );
    Ok(())
}

#[tokio::test]
async fn total_estimate_never_reads_a_composition_to_count() -> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.5.2: _total=estimate, "a rough estimate of the
    // number of matching resources is sufficient"; a node search answers its
    // row count (#661).
    let harness = searching().await;
    let composition = harness.composition();
    let node = super::diagnosis(&composition);
    let second = format!("{OTHER_CONTAINER}::ferrobridge.test::1");
    mount_rows(
        &harness.cdr,
        &[
            super::node_row(VERSION_ONE, &composition, &node),
            super::node_row(&second, &composition, &node),
        ],
    )
    .await;
    mount_count(&harness.cdr, &[composition.clone(), composition]).await;
    let (status, body) = get(&harness, "Condition?code=SYN-001&_count=1&_total=estimate").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some(2), body["total"].as_u64(), "{body}");
    let sent = queries(&harness).await;
    let counted = counts(&sent);
    assert_eq!(1, counted.len(), "{sent:?}");
    let text = counted.first().copied().ok_or("the count ran")?;
    assert!(text.contains("CONTAINS EVALUATION n0["), "{text}");
    assert!(!text.contains("AS composition"), "{text}");
    assert_eq!(
        2,
        sent.len(),
        "the page and the count are the only queries: {sent:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_cursor_of_another_search_is_refused() -> Result<(), Box<dyn StdError>> {
    let harness = searching().await;
    let (status, body) = get(
        &harness,
        "Condition?patient=p-1&_cursor=0.1.0.0000000000000000",
    )
    .await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    let issue = first_issue(&body);
    assert_eq!(Some("invalid"), issue["code"].as_str());
    assert_eq!(queries(&harness).await, Vec::<serde_json::Value>::new());
    let (status, _) = get(&harness, "Condition?patient=p-1&_cursor=nonsense").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status);
    Ok(())
}
