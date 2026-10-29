// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Each entry of a composition is a resource of its own, and a search
//! answers the entry that matched (#530).
//!
//! R4 search.html §3.1.1: the server returns the resources that match, so a
//! composition holding two diagnoses matched on its second answers the second
//! diagnosis with its own id. The stub CDR evaluates no AQL, so each row
//! names the node that matched as the real CDR would.

use ferrobridge_server::facade::identity::FhirResourceId;
use ferrobridge_server::facade::identity::derive;
use ferrobridge_server::facade::identity::store::Store;
use http::StatusCode;
use std::collections::BTreeSet;
use std::error::Error;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::super::VERSION_ONE;
use super::super::condition;
use super::super::stub::mount_read;
use super::AQL;
use super::DIAGNOSIS;
use super::OTHER_CONTAINER;
use super::counts;
use super::diagnosis;
use super::get;
use super::link;
use super::matches;
use super::mount_count;
use super::mount_rows;
use super::node_row;
use super::queries;
use super::result_set;
use super::row;
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
-> Result<(), Box<dyn Error>> {
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
async fn both_entries_matching_answer_two_resources() -> Result<(), Box<dyn Error>> {
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
async fn a_page_boundary_inside_a_composition_answers_each_entry_once() -> Result<(), Box<dyn Error>>
{
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

/// Returns the version uid of a second composition.
fn second_version() -> String {
    format!("{OTHER_CONTAINER}::ferrobridge.test::1")
}

#[tokio::test]
async fn a_search_binding_no_node_answers_every_entry_of_each_composition()
-> Result<(), Box<dyn Error>> {
    // #658, R4 search.html §3.1.1: every resource that meets the criteria is
    // in the searchset, so a subject search answers both diagnoses of each
    // composition, each with its own id.
    for query in [
        "Condition?patient=p-1",
        "Condition?context-start=ge2000-01-01",
    ] {
        let harness = searching().await;
        let (composition, _, _) = two_diagnoses(&harness);
        let second = second_version();
        mount_rows(
            &harness.cdr,
            &[row(VERSION_ONE, &composition), row(&second, &composition)],
        )
        .await;
        let (status, body) = get(&harness, query).await?;
        assert_eq!(StatusCode::OK, status, "{query}: {body}");
        let found = matches(&body);
        let codes: Vec<Option<&str>> = found.iter().map(|entry| code_of(entry)).collect();
        assert_eq!(
            vec![
                Some("SYN-001"),
                Some("SYN-002"),
                Some("SYN-001"),
                Some("SYN-002")
            ],
            codes,
            "{query}: {body}"
        );
        let ids: BTreeSet<&str> = found.iter().filter_map(|entry| id_of(entry)).collect();
        assert_eq!(4, ids.len(), "{query}: each entry has its own id: {body}");
        assert_eq!(Some(4), body["total"].as_u64(), "{query}: {body}");
    }
    Ok(())
}

#[tokio::test]
async fn paging_a_search_binding_no_node_answers_each_entry_once() -> Result<(), Box<dyn Error>> {
    // One composition, two entries, pages of one: page two starts at the
    // composition's row and skips the entry page one answered.
    let harness = searching().await;
    let (composition, _, _) = two_diagnoses(&harness);
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    mount_count(&harness.cdr, std::slice::from_ref(&composition)).await;
    let (status, page_one) =
        get(&harness, "Condition?patient=p-1&_count=1&_total=accurate").await?;
    assert_eq!(StatusCode::OK, status, "{page_one}");
    assert_eq!(
        Some(2),
        page_one["total"].as_u64(),
        "the total counts entries, never compositions: {page_one}"
    );
    let first = matches(&page_one);
    assert_eq!(1, first.len(), "{page_one}");
    assert_eq!(
        Some("SYN-001"),
        first.first().and_then(|entry| code_of(entry))
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
    let second = matches(&page_two);
    assert_eq!(1, second.len(), "{page_two}");
    assert_eq!(
        Some("SYN-002"),
        second.first().and_then(|entry| code_of(entry))
    );
    assert_ne!(
        first.first().and_then(|entry| id_of(entry)),
        second.first().and_then(|entry| id_of(entry)),
        "one entry answered on two pages"
    );
    assert_eq!(None, link(&page_two, "next"));
    Ok(())
}

#[tokio::test]
async fn a_count_of_a_search_binding_no_node_counts_entries() -> Result<(), Box<dyn Error>> {
    // R4 search.html §3.1.1.5.3: _count=0 reports in Bundle.total the number
    // of resources that match, here three entries in two compositions.
    // The count is one AQL query over the entry and reads no composition (#661).
    let harness = searching().await;
    let (two, _, _) = two_diagnoses(&harness);
    let one = harness.composition();
    let second = second_version();
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &two), row(&second, &one)]).await;
    mount_count(&harness.cdr, &[two, one]).await;
    let (status, body) = get(&harness, "Condition?patient=p-1&_count=0").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some(3), body["total"].as_u64(), "{body}");
    assert!(matches(&body).is_empty(), "{body}");
    let sent = queries(&harness).await;
    assert_eq!(1, sent.len(), "one count query and no row read: {sent:?}");
    let text = counts(&sent)
        .first()
        .copied()
        .ok_or("the count query ran")?;
    assert!(
        text.contains(&format!("CONTAINS EVALUATION x[{DIAGNOSIS}] WHERE")),
        "{text}"
    );
    assert!(!text.contains("AS composition"), "{text}");
    Ok(())
}

#[tokio::test]
async fn an_accurate_total_is_one_count_query_for_one_and_two_entry_compositions()
-> Result<(), Box<dyn Error>> {
    // R4 search.html §3.1.1.5.2: _total=accurate asks for the exact number;
    // a page that leaves a next page gets it from one count query (#661).
    for (two_entries, one_entry, expected) in [(0_usize, 2_usize, 2_u64), (2, 0, 4), (1, 1, 3)] {
        let harness = searching().await;
        let (two, _, _) = two_diagnoses(&harness);
        let one = harness.composition();
        let held: Vec<serde_json::Value> = std::iter::repeat_n(two, two_entries)
            .chain(std::iter::repeat_n(one, one_entry))
            .collect();
        let rows: Vec<serde_json::Value> = held
            .iter()
            .enumerate()
            .map(|(place, composition)| {
                let version = if place == 0 {
                    String::from(VERSION_ONE)
                } else {
                    second_version()
                };
                row(&version, composition)
            })
            .collect();
        mount_rows(&harness.cdr, &rows).await;
        mount_count(&harness.cdr, &held).await;
        let (status, body) =
            get(&harness, "Condition?patient=p-1&_count=1&_total=accurate").await?;
        assert_eq!(StatusCode::OK, status, "{body}");
        assert_eq!(1, matches(&body).len(), "{body}");
        assert_eq!(Some(expected), body["total"].as_u64(), "{body}");
        let sent = queries(&harness).await;
        assert_eq!(1, counts(&sent).len(), "{sent:?}");
        assert_eq!(
            1,
            sent.len().saturating_sub(counts(&sent).len()),
            "the page is the only row read: {sent:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_later_entry_is_bound_by_the_positional_path_of_its_place_in_content()
-> Result<(), Box<dyn Error>> {
    // openEHR BASE Release 1.2.0, Paths and Locators, §Using Positional
    // Parameters: the second diagnosis is the second `content` item.
    let harness = searching().await;
    let (composition, _first, second) = two_diagnoses(&harness);
    mount_rows(
        &harness.cdr,
        &[node_row(VERSION_ONE, &composition, &second)],
    )
    .await;
    let (status, body) = get(&harness, "Condition?code=SYN-002").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let entry = matches(&body).first().copied().ok_or("one match")?;
    let id = FhirResourceId::new(id_of(entry).ok_or("the match has an id")?)?;
    let binding = harness
        .store
        .binding_of("Condition", &id)?
        .ok_or("the entry is bound")?;
    assert_eq!("/content[2]", binding.entry_path);
    assert_eq!(Some(derive::DERIVATION), binding.derivation);
    Ok(())
}

/// Returns the ids of the `Condition`s `$tofhir` answers for `composition`,
/// run by the operations lane over the mapping set under `fixtures` and the
/// operational template `template`.
pub(super) async fn tofhir_ids(
    fixtures: &str,
    template: &str,
    composition: &serde_json::Value,
) -> Result<Vec<String>, Box<dyn Error>> {
    let answer = super::super::tofhir(fixtures, template, composition).await?;
    Ok(super::super::ids_of_type(&answer, "Condition"))
}

// NOTE: FHIRconnect v1.0.0 `engine/id-management.adoc` §Deterministic IDs: one
// occurrence takes one id, the hash of the composition uid and the entry path.
#[tokio::test]
async fn the_facade_read_its_search_and_tofhir_give_each_occurrence_one_id()
-> Result<(), Box<dyn Error>> {
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
    let searched: Vec<String> = matches(&body)
        .iter()
        .filter_map(|entry| id_of(entry).map(str::to_owned))
        .collect();
    assert_eq!(2, searched.len(), "{body}");

    mount_read(&harness.cdr, VERSION_ONE, &composition).await;
    for id in &searched {
        let (status, read) = get(&harness, &format!("Condition/{id}")).await?;
        assert_eq!(StatusCode::OK, status, "{read}");
        assert_eq!(Some(id.as_str()), read["id"].as_str());
    }

    let mut held = composition.clone();
    held["uid"] = serde_json::json!({ "_type": "OBJECT_VERSION_ID", "value": VERSION_ONE });
    let answered = tofhir_ids(
        super::super::FIXTURES,
        ferrobridge_testkit::fixtures::DIAGNOSES_OPT,
        &held,
    )
    .await?;
    assert_eq!(searched, answered);
    Ok(())
}
