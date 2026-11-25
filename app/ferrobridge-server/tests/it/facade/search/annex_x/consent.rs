// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The advance directives of draft `EEHRxF` Annex X beside the access
//! directives the facade serves as `Consent`: a search without `scope`
//! answers both, paged as one searchset, a `scope` picks the lane its code
//! names, and a read reaches the lane its id belongs to.

use std::error::Error;

use axum::body::Body;
use ferrobridge_server::facade::endpoint::Endpoints;
use http::Request;
use http::StatusCode;
use http::header;
use openehr_rm::draft::access_control::terminology::AccessDefault;
use serde_json::Value;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::super::super::BASE_URL;
use super::super::super::EHR_ID;
use super::super::super::Harness;
use super::super::super::VERSION_ONE;
use super::super::super::call;
use super::super::super::stub::mount_ehr;
use super::super::super::stub::mount_read;
use super::super::SUBJECT;
use super::super::get;
use super::super::matches;
use super::super::mount_count;
use super::super::mount_rows;
use super::EXTENSIONS;
use super::annex_x_over;
use super::composition_of;
use super::entry_row;
use super::group_named;

/// An `EHR_ACCESS` with no settings, the state of a new EHR.
const EMPTY_ACCESS: &str = r#"{
  "_type": "EHR_ACCESS",
  "name": {"_type": "DV_TEXT", "value": "EHR access"},
  "archetype_node_id": "openEHR-EHR-EHR_ACCESS.access_directives.v1.0.0"
}"#;

/// The `EHR_ACCESS` version the stub serves before the commit.
const ACCESS_ONE: &str = "0a1b2c3d-0000-4000-8000-00000000000b::ferrobridge.test::1";

/// The `EHR_ACCESS` version the stub serves after the commit.
const ACCESS_TWO: &str = "0a1b2c3d-0000-4000-8000-00000000000b::ferrobridge.test::2";

/// Mounts the `EHR_ACCESS` read answering `access` as `version`, ahead of
/// any earlier one when `priority` is lower.
async fn serve_access(harness: &Harness, access: &str, version: &str, priority: u8) {
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "{EXTENSIONS}/ehr/{EHR_ID}/ehr_access"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", format!("\"{version}\"").as_str())
                .set_body_raw(access.to_owned(), "application/json"),
        )
        .with_priority(priority)
        .mount(&harness.cdr)
        .await;
}

/// Returns the Annex X harness serving the access directives, with one
/// directive in force for the suite's patient and one advance directive.
async fn both_lanes() -> Result<Harness, Box<dyn Error>> {
    let harness = annex_x_over(Endpoints::default(), Some(AccessDefault::Restricted)).await?;
    let group = group_named("EHDSAdvanceDirective")?;
    let composition = composition_of(&harness, &group);
    mount_rows(&harness.cdr, &[entry_row(&composition, group.archetype)?]).await;
    mount_count(&harness.cdr, std::slice::from_ref(&composition)).await;
    mount_read(&harness.cdr, VERSION_ONE, &composition).await;
    mount_ehr(&harness.cdr).await;
    serve_access(&harness, EMPTY_ACCESS, ACCESS_ONE, 5).await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!("{EXTENSIONS}/admin/access_policy")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&harness.cdr)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path(format!("/ehr/{EHR_ID}/contribution")))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("ETag", "\"6c7e5d4b-0000-4000-8000-000000000006\""),
        )
        .mount(&harness.cdr)
        .await;
    let mut consent: Value = serde_json::from_slice(&crate::consent::fhir::vendored(
        ferrobridge_server::consent::fhir::Release::R4,
        "consent-example-notTime.json",
    ))?;
    consent["patient"] = serde_json::json!({ "reference": SUBJECT });
    let (status, created) = call(
        harness.app(),
        Request::post("/fhir/Consent")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(consent.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{created}");
    let requests = harness.cdr.received_requests().await.ok_or("recorded")?;
    let commit = requests
        .iter()
        .rev()
        .find(|request| {
            request.method.as_str() == "POST" && request.url.path().ends_with("/contribution")
        })
        .ok_or("a commit reached the CDR")?;
    let contribution: Value = serde_json::from_slice(&commit.body)?;
    let access = contribution
        .pointer("/versions/0/data")
        .ok_or("the version carries the EHR_ACCESS")?;
    serve_access(&harness, &access.to_string(), ACCESS_TWO, 1).await;
    Ok(harness)
}

/// Returns the `Consent.scope` code of each match of `body`, in order.
fn scopes(body: &Value) -> Vec<&str> {
    matches(body)
        .iter()
        .filter_map(|entry| entry["resource"]["scope"]["coding"][0]["code"].as_str())
        .collect()
}

/// Returns the `next` link of `body` relative to the FHIR base.
fn next_of(body: &Value) -> Option<String> {
    body["link"].as_array()?.iter().find_map(|link| {
        (link["relation"] == "next")
            .then(|| link["url"].as_str())
            .flatten()
            .and_then(|url| url.strip_prefix(&format!("{BASE_URL}/")))
            .map(str::to_owned)
    })
}

#[tokio::test]
async fn a_search_without_scope_answers_the_access_and_the_advance_directives()
-> Result<(), Box<dyn Error>> {
    let harness = both_lanes().await?;

    let (status, body) = get(&harness, &format!("Consent?patient={SUBJECT}")).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(vec!["patient-privacy", "adr"], scopes(&body), "{body}");
    assert_eq!(Some(2), body["total"].as_u64(), "{body}");

    let (status, form) = call(
        harness.app(),
        Request::post("/fhir/Consent/_search")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("patient={SUBJECT}")))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{form}");
    assert_eq!(vec!["patient-privacy", "adr"], scopes(&form), "{form}");
    Ok(())
}

#[tokio::test]
async fn a_search_of_both_lanes_pages_across_them() -> Result<(), Box<dyn Error>> {
    let harness = both_lanes().await?;

    let (status, first) = get(&harness, &format!("Consent?patient={SUBJECT}&_count=1")).await?;
    assert_eq!(StatusCode::OK, status, "{first}");
    assert_eq!(vec!["patient-privacy"], scopes(&first), "{first}");
    let next = next_of(&first).ok_or(format!("a next link in {first}"))?;

    let (status, second) = get(&harness, &next).await?;
    assert_eq!(StatusCode::OK, status, "{second}");
    assert_eq!(vec!["adr"], scopes(&second), "{second}");
    assert_eq!(None, next_of(&second), "{second}");

    let (status, counted) = get(
        &harness,
        &format!("Consent?patient={SUBJECT}&_summary=count"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{counted}");
    assert_eq!(Some(2), counted["total"].as_u64(), "{counted}");
    Ok(())
}

#[tokio::test]
async fn a_scope_picks_the_lane_its_code_names() -> Result<(), Box<dyn Error>> {
    let harness = both_lanes().await?;
    for (scope, expected) in [
        ("adr", vec!["adr"]),
        ("patient-privacy", vec!["patient-privacy"]),
        (
            "http://terminology.hl7.org/CodeSystem/consentscope|patient-privacy",
            vec!["patient-privacy"],
        ),
        ("adr,patient-privacy", vec!["patient-privacy", "adr"]),
        ("research", Vec::new()),
    ] {
        let (status, body) = get(
            &harness,
            &format!("Consent?patient={SUBJECT}&scope={}", urlencoding(scope)),
        )
        .await?;
        assert_eq!(StatusCode::OK, status, "{scope}: {body}");
        assert_eq!(expected, scopes(&body), "{scope}: {body}");
    }
    Ok(())
}

#[tokio::test]
async fn a_search_of_both_lanes_refuses_what_the_access_directives_cannot_match()
-> Result<(), Box<dyn Error>> {
    let harness = both_lanes().await?;
    let (status, body) = get(&harness, &format!("Consent?patient={SUBJECT}&date=ge2020")).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    assert_eq!(
        Some("date"),
        body["issue"][0]["location"][0].as_str(),
        "{body}"
    );

    let (status, body) = get(
        &harness,
        &format!("Consent?patient={SUBJECT}&scope=adr&date=ge2020"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn a_read_reaches_the_lane_its_id_belongs_to() -> Result<(), Box<dyn Error>> {
    let harness = both_lanes().await?;
    let (status, body) = get(&harness, &format!("Consent?patient={SUBJECT}")).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let found = matches(&body);
    for entry in &found {
        let id = entry["resource"]["id"].as_str().ok_or("an id")?;
        let (status, read) = get(&harness, &format!("Consent/{id}")).await?;
        assert_eq!(StatusCode::OK, status, "{id}: {read}");
        assert_eq!(entry["resource"]["scope"], read["scope"], "{id}: {read}");
    }
    assert_eq!(2, found.len(), "{body}");
    Ok(())
}

#[tokio::test]
async fn an_advance_directive_is_searched_by_its_scope_beside_the_access_directives()
-> Result<(), Box<dyn Error>> {
    let harness = both_lanes().await?;

    let (status, statement) = get(&harness, "metadata").await?;
    assert_eq!(StatusCode::OK, status, "{statement}");
    let consents: Vec<&Value> = statement["rest"][0]["resource"]
        .as_array()
        .map(|resources| {
            resources
                .iter()
                .filter(|resource| resource["type"] == "Consent")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(1, consents.len(), "{statement}");
    let declared = |name: &str| {
        consents[0]["searchParam"]
            .as_array()
            .is_some_and(|params| params.iter().any(|param| param["name"] == name))
    };
    assert!(declared("scope") && declared("patient"), "{statement}");
    assert!(
        consents[0]["operation"]
            .as_array()
            .is_some_and(|operations| operations
                .iter()
                .any(|operation| operation["name"] == "bppc")),
        "{statement}"
    );
    Ok(())
}

/// Form-encodes one query value.
fn urlencoding(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
