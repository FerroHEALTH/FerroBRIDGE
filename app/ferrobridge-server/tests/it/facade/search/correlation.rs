// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A token's code and system are read from one node, and an entry with
//! several matching nodes is one match (#526).
//!
//! R4 matches `[system]|[code]` against one `Coding`
//! (<https://hl7.org/fhir/R4/search.html#token>). The stub CDR evaluates no
//! AQL, so these cases pin the query the facade writes and how it reads a
//! result with a row per matching node; the real-CDR case behind
//! `FERROBRIDGE_E2E` proves the crossed pair matches nothing.

use http::StatusCode;
use std::error::Error;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::super::VERSION_ONE;
use super::AQL;
use super::OTHER_CONTAINER;
use super::bound;
use super::counts;
use super::diagnosis;
use super::get;
use super::link;
use super::matches;
use super::mount_rows;
use super::node_row;
use super::queries;
use super::result_set;
use super::searching;

#[tokio::test]
async fn code_and_system_are_compared_on_the_one_node_the_from_binds() -> Result<(), Box<dyn Error>>
{
    let harness = searching().await;
    mount_rows(&harness.cdr, &[]).await;
    let (status, body) = get(&harness, "Condition?code=http://snomed.info/sct%7C44054006").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let sent = queries(&harness).await;
    let query = sent.first().ok_or("the search reached the CDR")?;
    let text = query["q"].as_str().unwrap_or_default();
    assert!(
        text.contains(
            "CONTAINS COMPOSITION c CONTAINS EVALUATION n0[openEHR-EHR-EVALUATION.problem_diagnosis.v1] WHERE"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "((n0/data[at0001]/items[at0002]/value/defining_code/terminology_id/value = $p0 AND n0/data[at0001]/items[at0002]/value/defining_code/code_string = $p1))"
        ),
        "the code and the system are not one node's: {text}"
    );
    assert!(
        !text.contains("c/content["),
        "a path still crosses the repeated node: {text}"
    );
    assert_eq!(
        vec![
            String::from("SNOMED-CT"),
            String::from("44054006"),
            String::from("ferrobridge.diagnoses.v1"),
        ],
        bound(query)
    );
    Ok(())
}

#[tokio::test]
async fn a_composition_whose_two_nodes_match_is_one_entry() -> Result<(), Box<dyn Error>> {
    let harness = searching().await;
    let composition = harness.composition();
    let node = diagnosis(&composition);
    mount_rows(
        &harness.cdr,
        &[
            node_row(VERSION_ONE, &composition, &node),
            node_row(VERSION_ONE, &composition, &node),
        ],
    )
    .await;
    let (status, body) = get(&harness, "Condition?code=SYN-001,SYN-002").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(1, matches(&body).len(), "{body}");
    assert_eq!(
        Some(1),
        body["total"].as_u64(),
        "the total counts compositions: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn paging_and_the_total_count_compositions_and_never_nodes() -> Result<(), Box<dyn Error>> {
    // Rows A, A, B: two compositions, the first matching on two nodes. A page
    // of one holds A, and the next page starts at B's row, offset 2.
    let harness = searching().await;
    let composition = harness.composition();
    let node = diagnosis(&composition);
    let second = format!("{OTHER_CONTAINER}::ferrobridge.test::1");
    let answer =
        |rows: &[serde_json::Value]| ResponseTemplate::new(200).set_body_json(result_set(rows));
    for (offset, fetch, rows) in [
        (
            0,
            2,
            vec![
                node_row(VERSION_ONE, &composition, &node),
                node_row(VERSION_ONE, &composition, &node),
            ],
        ),
        (2, 1, vec![node_row(&second, &composition, &node)]),
        (
            0,
            100,
            vec![
                node_row(VERSION_ONE, &composition, &node),
                node_row(VERSION_ONE, &composition, &node),
                node_row(&second, &composition, &node),
            ],
        ),
        (2, 2, vec![node_row(&second, &composition, &node)]),
    ] {
        Mock::given(matchers::method("POST"))
            .and(matchers::path(AQL))
            .and(matchers::body_partial_json(
                serde_json::json!({ "offset": offset, "fetch": fetch }),
            ))
            .respond_with(answer(&rows))
            .mount(&harness.cdr)
            .await;
    }
    let (status, first) = get(&harness, "Condition?code=SYN-001&_count=1&_total=accurate").await?;
    assert_eq!(StatusCode::OK, status, "{first}");
    assert_eq!(1, matches(&first).len(), "{first}");
    assert_eq!(Some(2), first["total"].as_u64(), "{first}");
    assert!(
        counts(&queries(&harness).await).is_empty(),
        "a node search counts its matches from the rows"
    );
    let next = link(&first, "next").ok_or("a second composition is left")?;
    assert!(
        next.contains("_cursor=0.2."),
        "the next page starts at B's row: {next}"
    );
    let (status, second_page) = get(
        &harness,
        next.strip_prefix("http://ferrobridge.invalid/fhir/")
            .unwrap_or(next),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{second_page}");
    assert_eq!(1, matches(&second_page).len(), "{second_page}");
    assert_eq!(None, link(&second_page, "next"));
    let ids: Vec<&str> = [&first, &second_page]
        .iter()
        .filter_map(|page| {
            matches(page)
                .first()
                .and_then(|entry| entry["resource"]["id"].as_str())
        })
        .collect();
    assert_eq!(2, ids.len());
    assert_ne!(
        ids.first(),
        ids.get(1),
        "a composition answered on two pages"
    );
    let counts: Vec<String> = queries(&harness)
        .await
        .iter()
        .filter_map(|query| query["q"].as_str().map(str::to_owned))
        .filter(|text| text.contains("COUNT("))
        .collect();
    assert!(
        counts.is_empty(),
        "a total over a bound node counts the entries its rows answer (#530), never a COUNT of version uids: {counts:?}"
    );
    Ok(())
}
