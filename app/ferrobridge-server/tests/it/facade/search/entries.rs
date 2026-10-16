// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Each entry of a composition is a resource of its own, and a search
//! answers the entry that matched (#530).
//!
//! R4 search.html §3.1.1: the server returns the resources that match, so a
//! composition holding two diagnoses matched on its second answers the second
//! diagnosis with its own id. The stub CDR evaluates no AQL, so each row
//! names the node that matched as the real CDR would.

use http::StatusCode;
use std::error::Error as StdError;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::super::VERSION_ONE;
use super::super::condition;
use super::super::stub::mount_read;
use super::AQL;
use super::diagnosis;
use super::get;
use super::link;
use super::matches;
use super::mount_rows;
use super::node_row;
use super::result_set;
use super::searching;

/// Returns the composition one synthetic `Condition` coded `code` maps to.
fn coded(harness: &super::super::Harness, code: &str) -> serde_json::Value {
    let mut resource = condition();
    resource["code"]["coding"][0]["code"] = serde_json::json!(code);
    harness.composition_of(&resource)
}

/// Returns a composition holding two diagnoses, `SYN-001` then `SYN-002`,
/// with each diagnosis entry.
fn two_diagnoses(
    harness: &super::super::Harness,
) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    let mut composition = coded(harness, "SYN-001");
    let first = diagnosis(&composition);
    let second = diagnosis(&coded(harness, "SYN-002"));
    composition["content"]
        .as_array_mut()
        .expect("the composition has content")
        .push(second.clone());
    (composition, first, second)
}

/// Returns the code of the resource one match carries.
fn code_of(entry: &serde_json::Value) -> Option<&str> {
    entry["resource"]["code"]["coding"][0]["code"].as_str()
}

/// Returns the logical id of the resource one match carries.
fn id_of(entry: &serde_json::Value) -> Option<&str> {
    entry["resource"]["id"].as_str()
}

#[tokio::test]
async fn a_match_on_the_second_entry_answers_that_entry_with_its_own_id()
-> Result<(), Box<dyn StdError>> {
    let harness = searching().await;
    let (composition, first, second) = two_diagnoses(&harness);
    for (code, node) in [("SYN-001", &first), ("SYN-002", &second)] {
        Mock::given(matchers::method("POST"))
            .and(matchers::path(AQL))
            .and(matchers::body_string_contains(format!("\"{code}\"")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(result_set(&[node_row(
                    VERSION_ONE,
                    &composition,
                    node,
                )])),
            )
            .mount(&harness.cdr)
            .await;
    }
    let (status, body) = get(&harness, "Condition?code=SYN-002").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let found = matches(&body);
    assert_eq!(1, found.len(), "{body}");
    let entry = found.first().ok_or("one match")?;
    assert_eq!(
        Some("SYN-002"),
        code_of(entry),
        "the match carries the first entry's code: {body}"
    );
    let second_id = id_of(entry).ok_or("the match has an id")?.to_owned();

    let (status, other) = get(&harness, "Condition?code=SYN-001").await?;
    assert_eq!(StatusCode::OK, status, "{other}");
    let first_match = matches(&other).first().copied().ok_or("one match")?;
    assert_eq!(Some("SYN-001"), code_of(first_match), "{other}");
    assert_ne!(
        Some(second_id.as_str()),
        id_of(first_match),
        "two entries answered one id"
    );

    mount_read(&harness.cdr, VERSION_ONE, &composition).await;
    let (status, read) = get(&harness, &format!("Condition/{second_id}")).await?;
    assert_eq!(StatusCode::OK, status, "{read}");
    assert_eq!(Some(second_id.as_str()), read["id"].as_str());
    assert_eq!(
        Some("SYN-002"),
        read["code"]["coding"][0]["code"].as_str(),
        "a read of the second entry's id answers the first entry: {read}"
    );
    Ok(())
}

#[tokio::test]
async fn both_entries_matching_answer_two_resources() -> Result<(), Box<dyn StdError>> {
    let harness = searching().await;
    let (composition, first, second) = two_diagnoses(&harness);
    mount_rows(
        &harness.cdr,
        &[
            node_row(VERSION_ONE, &composition, &first),
            node_row(VERSION_ONE, &composition, &second),
        ],
    )
    .await;
    let (status, body) = get(&harness, "Condition?code=SYN-001,SYN-002").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let found = matches(&body);
    let codes: Vec<Option<&str>> = found.iter().map(|entry| code_of(entry)).collect();
    assert_eq!(vec![Some("SYN-001"), Some("SYN-002")], codes, "{body}");
    let ids: Vec<Option<&str>> = found.iter().map(|entry| id_of(entry)).collect();
    assert_eq!(2, ids.len());
    assert_ne!(ids.first(), ids.get(1), "two entries answered one id");
    assert_eq!(Some(2), body["total"].as_u64(), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_page_boundary_inside_a_composition_answers_each_entry_once()
-> Result<(), Box<dyn StdError>> {
    // One composition, two matching entries, pages of one: the next page
    // starts at the composition's first row and skips the entry page one
    // answered.
    let harness = searching().await;
    let (composition, first, second) = two_diagnoses(&harness);
    let both = vec![
        node_row(VERSION_ONE, &composition, &first),
        node_row(VERSION_ONE, &composition, &second),
    ];
    for (offset, fetch, rows) in [
        (0, 2, both.clone()),
        (0, 100, both.clone()),
        (2, 1, Vec::new()),
    ] {
        Mock::given(matchers::method("POST"))
            .and(matchers::path(AQL))
            .and(matchers::body_partial_json(
                serde_json::json!({ "offset": offset, "fetch": fetch }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(result_set(&rows)))
            .mount(&harness.cdr)
            .await;
    }
    let (status, page_one) = get(
        &harness,
        "Condition?code=SYN-001,SYN-002&_count=1&_total=accurate",
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{page_one}");
    assert_eq!(
        Some(2),
        page_one["total"].as_u64(),
        "the total counts entries: {page_one}"
    );
    let found = matches(&page_one);
    assert_eq!(1, found.len(), "{page_one}");
    assert_eq!(
        Some("SYN-001"),
        found.first().and_then(|entry| code_of(entry))
    );
    let next = link(&page_one, "next").ok_or("a second entry is left")?;
    assert!(
        next.contains("_cursor=0.0.1."),
        "the next page starts at the composition and skips one entry: {next}"
    );
    let (status, page_two) = get(
        &harness,
        next.strip_prefix("http://ferrobridge.invalid/fhir/")
            .unwrap_or(next),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{page_two}");
    let found = matches(&page_two);
    assert_eq!(1, found.len(), "{page_two}");
    assert_eq!(
        Some("SYN-002"),
        found.first().and_then(|entry| code_of(entry))
    );
    assert_eq!(None, link(&page_two, "next"));
    Ok(())
}
