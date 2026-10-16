// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The searchset on the wire: its shape, the empty answers, the
//! `CapabilityStatement`, the two request forms, and the CDR's failures.

use axum::body::Body;
use http::Request;
use http::StatusCode;
use http::header;
use std::error::Error as StdError;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::super::EHR_ID;
use super::super::VERSION_ONE;
use super::super::call;
use super::super::first_issue;
use super::super::raw;
use super::AQL;
use super::OTHER_CONTAINER;
use super::bound;
use super::get;
use super::link;
use super::matches;
use super::mount_rows;
use super::queries;
use super::row;
use super::searching;

#[tokio::test]
async fn a_search_answers_one_entry_per_composition_in_a_well_formed_searchset()
-> Result<(), Box<dyn StdError>> {
    // R4 bundle.html bdl-7 (unique fullUrl), search.html §3.1.1.5.2 (total
    // counts matches) and §3.1.1.5.4 (entry.search.mode); the carry-over
    // register adds Bundle.id and meta, which the retired connector lacked.
    let harness = searching().await;
    let composition = harness.composition();
    let second = format!("{OTHER_CONTAINER}::ferrobridge.test::1");
    mount_rows(
        &harness.cdr,
        &[row(VERSION_ONE, &composition), row(&second, &composition)],
    )
    .await;
    let response = raw(
        harness.app(),
        Request::get("/fhir/Condition?patient=p-1").body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        Some("application/fhir+json"),
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(Some("Bundle"), body["resourceType"].as_str());
    assert_eq!(Some("searchset"), body["type"].as_str());
    assert!(
        body["id"].as_str().is_some_and(|id| !id.is_empty()),
        "{body}"
    );
    assert!(body["meta"]["lastUpdated"].as_str().is_some(), "{body}");
    assert_eq!(Some(2), body["total"].as_u64(), "{body}");
    let found = matches(&body);
    assert_eq!(2, found.len(), "{body}");
    let urls: Vec<&str> = found
        .iter()
        .filter_map(|entry| entry["fullUrl"].as_str())
        .collect();
    assert_eq!(2, urls.len());
    assert_ne!(urls.first(), urls.get(1), "two entries share a fullUrl");
    for entry in &found {
        let id = entry["resource"]["id"]
            .as_str()
            .expect("each resource has an id");
        assert_eq!(
            Some(format!("http://ferrobridge.invalid/fhir/Condition/{id}").as_str()),
            entry["fullUrl"].as_str()
        );
        assert_eq!(
            Some("Condition"),
            entry["resource"]["resourceType"].as_str()
        );
        assert!(
            entry["resource"]["subject"]["reference"].as_str() == Some("Patient/p-1"),
            "the subject comes from the identity map: {entry}"
        );
    }
    assert_eq!(
        Some("http://ferrobridge.invalid/fhir/Condition?patient=p-1&_count=20"),
        link(&body, "self")
    );
    assert_eq!(None, link(&body, "next"));
    let sent = queries(&harness).await;
    let query = sent.first().expect("one row query reached the CDR");
    assert_eq!(1, sent.len());
    let text = query["q"].as_str().unwrap_or_default();
    assert!(
        text.contains("CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c"),
        "{text}"
    );
    assert!(
        text.contains("c/archetype_details/template_id/value = $template"),
        "{text}"
    );
    assert!(
        text.contains("ORDER BY v/commit_audit/time_committed/value ASC, v/uid/value ASC"),
        "{text}"
    );
    let values = bound(query);
    assert!(
        values.contains(&String::from("ferrobridge.diagnose.v1")),
        "{values:?}"
    );
    assert!(values.contains(&String::from(EHR_ID)), "{values:?}");
    assert_eq!(Some(0), query["offset"].as_i64());
    assert_eq!(
        Some(21),
        query["fetch"].as_i64(),
        "a page reads one row past its size"
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_patient_is_an_empty_searchset_and_never_a_404() -> Result<(), Box<dyn StdError>>
{
    // R4 search.html §3.1.1.3: an unknown subject is a logical condition, so
    // the search answers an empty set; the identity map decides, never the CDR.
    let harness = searching().await;
    let (status, body) = get(&harness, "Condition?patient=nobody").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some("searchset"), body["type"].as_str());
    assert_eq!(Some(0), body["total"].as_u64());
    assert!(matches(&body).is_empty());
    assert!(
        queries(&harness).await.is_empty(),
        "an unknown subject reached the CDR"
    );
    Ok(())
}

#[tokio::test]
async fn an_empty_store_answers_two_hundred_with_the_fhir_media_type()
-> Result<(), Box<dyn StdError>> {
    let harness = searching().await;
    mount_rows(&harness.cdr, &[]).await;
    let response = raw(
        harness.app(),
        Request::get("/fhir/Condition").body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        Some("application/fhir+json"),
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(Some(0), body["total"].as_u64(), "{body}");
    assert!(body.get("entry").is_none() || matches(&body).is_empty());
    Ok(())
}

#[tokio::test]
async fn a_post_search_reads_its_form_body_as_a_get_reads_its_query()
-> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.2: a POST _search carries the parameters as an
    // application/x-www-form-urlencoded body.
    let harness = searching().await;
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &harness.composition())]).await;
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir/Condition/_search")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("patient=Patient%2Fp-1"))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(1, matches(&body).len());
    assert_eq!(
        Some("http://ferrobridge.invalid/fhir/Condition?patient=Patient%2Fp-1&_count=20"),
        link(&body, "self")
    );
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir/Condition/_search")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from("{}"))?,
    )
    .await?;
    assert_eq!(StatusCode::UNSUPPORTED_MEDIA_TYPE, status, "{body}");
    assert_eq!(Some("not-supported"), first_issue(&body)["code"].as_str());
    Ok(())
}

#[tokio::test]
async fn a_type_without_a_search_declaration_answers_404_not_supported()
-> Result<(), Box<dyn StdError>> {
    let harness = super::super::harness().await;
    let (status, body) = get(&harness, "Condition").await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    assert_eq!(Some("not-supported"), first_issue(&body)["code"].as_str());
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_names_exactly_the_declared_parameters()
-> Result<(), Box<dyn StdError>> {
    // R4 capabilitystatement.html: rest.resource.searchParam names the
    // parameters the server supports, and search-type the interaction.
    let harness = searching().await;
    let (status, body) = get(&harness, "metadata").await?;
    assert_eq!(StatusCode::OK, status);
    let condition = &body["rest"][0]["resource"][0];
    let interactions: Vec<&str> = condition["interaction"]
        .as_array()
        .expect("the type names its interactions")
        .iter()
        .filter_map(|entry| entry["code"].as_str())
        .collect();
    assert_eq!(
        vec!["create", "read", "vread", "update", "search-type"],
        interactions
    );
    let parameters: Vec<(&str, &str)> = condition["searchParam"]
        .as_array()
        .expect("the type names its search parameters")
        .iter()
        .filter_map(|entry| Some((entry["name"].as_str()?, entry["type"].as_str()?)))
        .collect();
    assert_eq!(
        vec![
            ("_id", "token"),
            ("code", "token"),
            ("context-start", "date"),
            ("diagnosis-name", "string"),
            ("onset-date", "date"),
            ("patient", "reference"),
            ("subject", "reference"),
        ],
        parameters
    );
    assert_eq!(
        Some("http://hl7.org/fhir/SearchParameter/Resource-id"),
        condition["searchParam"][0]["definition"].as_str()
    );
    Ok(())
}

#[tokio::test]
async fn a_cdr_failure_maps_through_the_status_table_and_never_to_an_empty_bundle()
-> Result<(), Box<dyn StdError>> {
    // docs/architecture.md §4.6: any CDR 5xx is a 502 carrying the upstream
    // status; the query operation's own 400 is the bridge's fault, a 500.
    for (upstream, expected, code) in [
        (
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::BAD_GATEWAY,
            "transient",
        ),
        (
            StatusCode::BAD_REQUEST,
            StatusCode::INTERNAL_SERVER_ERROR,
            "exception",
        ),
        (
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::BAD_GATEWAY,
            "transient",
        ),
    ] {
        let harness = searching().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path(AQL))
            .respond_with(
                ResponseTemplate::new(upstream).set_body_json(serde_json::json!({
                    "message": "synthetic query refusal",
                    "validationErrors": []
                })),
            )
            .mount(&harness.cdr)
            .await;
        let (status, body) = get(&harness, "Condition?patient=p-1").await?;
        assert_eq!(expected, status, "{upstream}: {body}");
        let issue = first_issue(&body);
        assert_eq!(Some(code), issue["code"].as_str(), "{upstream}: {body}");
        assert!(
            issue["diagnostics"]
                .as_str()
                .is_some_and(|text| text.contains(upstream.as_str())),
            "{upstream}: the answer does not carry the upstream status: {body}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_row_the_bridge_cannot_read_is_a_502_naming_the_column_alone()
-> Result<(), Box<dyn StdError>> {
    let harness = searching().await;
    mount_rows(
        &harness.cdr,
        &[serde_json::json!([EHR_ID, "no-version-id", {"marker": "ferrobridge-synthetic-clinical-marker"}])],
    )
    .await;
    let (status, body) = get(&harness, "Condition?patient=p-1").await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status, "{body}");
    let text = body.to_string();
    assert!(text.contains("version_uid"), "{text}");
    assert!(
        !text.contains("ferrobridge-synthetic-clinical-marker"),
        "{text}"
    );
    Ok(())
}
