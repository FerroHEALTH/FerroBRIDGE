// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Sources pages: the list, one source with its versions and its edit
//! form, a create that answers `303` to the new source, a change sent under
//! `If-Match` on the `ETag` the page showed, a refused change drawn with the
//! fields kept, and a delete that answers `303` to the list.
//!
//! Every source is synthetic.

use std::error::Error;

use http::StatusCode;
use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::stub::{answer, bridge, post, signed_in};
use crate::support::{OPERATOR_TOKEN, get_with, send};

/// The id of the synthetic source.
const ID: &str = "01900000-0000-7000-8000-000000000001";

/// The synthetic source at version 2.
fn source() -> serde_json::Value {
    json!({
        "id": ID,
        "name": "device-a",
        "kind": "mllp",
        "enabled": true,
        "version": 2,
        "document": {"routes": [{"sending_application": "MONITOR", "sending_facility": "WARD-1", "source_cidrs": ["10.0.0.0/8"]}]},
        "updated_by": "operator-1",
        "updated_at": "2026-11-24T10:00:00.000000Z"
    })
}

/// Mounts the source and its versions on `bridge`.
async fn mount_source(bridge: &MockServer) {
    Mock::given(method("GET"))
        .and(path(format!("/operator/sources/{ID}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"2\"")
                .set_body_json(source()),
        )
        .mount(bridge)
        .await;
    answer(
        bridge,
        &format!("/operator/sources/{ID}/versions"),
        200,
        &json!({"versions": [
            {"version": 1, "name": "device-a", "enabled": false, "document": {}, "deleted": false,
             "committer": "operator-0", "committed_at": "2026-11-23T10:00:00.000000Z"},
            {"version": 2, "name": "device-a", "enabled": true, "document": {}, "deleted": false,
             "committer": "operator-1", "committed_at": "2026-11-24T10:00:00.000000Z"}
        ]}),
    )
    .await;
}

/// The form body of a save of `device-a` at the `ETag` `"2"`.
const SAVE: &str = "name=device-a&kind=mllp&enabled=on&etag=%222%22&decision=save&document=%7B%22routes%22%3A%5B%5D%7D";

#[tokio::test]
async fn the_list_names_every_source_with_its_state_and_version() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    answer(
        &bridge,
        "/operator/sources",
        200,
        &json!({"sources": [source()]}),
    )
    .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, get_with("/sources", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    for shown in [
        "device-a",
        "HL7 v2 over MLLP",
        "Enabled",
        "operator-1",
        &format!("/sources/{ID}"),
        "New source",
    ] {
        assert!(page.contains(shown), "{shown}: {page}");
    }
    Ok(())
}

#[tokio::test]
async fn a_bridge_without_a_state_store_says_so() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    answer(
        &bridge,
        "/operator/sources",
        404,
        &json!({"type": "about:blank", "title": "Not Found", "status": 404}),
    )
    .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/sources", &cookie)?).await?;
    assert!(
        page.contains("This bridge keeps no inbound sources"),
        "{page}"
    );
    Ok(())
}

#[tokio::test]
async fn a_source_shows_its_facts_versions_and_edit_form() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    mount_source(&bridge).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, get_with(&format!("/sources/{ID}"), &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    for shown in [
        "device-a",
        "operator-0",
        "name=\"etag\" value=\"&#34;2&#34;\"",
        "name=\"decision\" value=\"delete\"",
        "MONITOR",
    ] {
        assert!(page.contains(shown), "{shown}: {page}");
    }
    Ok(())
}

#[tokio::test]
async fn a_create_answers_303_to_the_new_source() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/operator/sources"))
        .and(header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .and(body_partial_json(
            json!({"name": "device-a", "kind": "mllp", "enabled": true}),
        ))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("ETag", "\"1\"")
                .set_body_json(source()),
        )
        .expect(1)
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, _page) = send(
        &service,
        post(
            "/sources/new",
            "name=device-a&kind=mllp&enabled=on&document=%7B%22routes%22%3A%5B%5D%7D",
            &cookie,
        )?,
    )
    .await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!(
        Some(format!("/sources/{ID}").as_str()),
        response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
    );
    Ok(())
}

#[tokio::test]
async fn a_document_that_is_no_json_never_asks_the_bridge() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/operator/sources"))
        .respond_with(ResponseTemplate::new(201))
        .expect(0)
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(
        &service,
        post(
            "/sources/new",
            "name=device-a&kind=mllp&document=%7Bnot",
            &cookie,
        )?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(page.contains("The document is no JSON object."), "{page}");
    assert!(
        page.contains("value=\"device-a\""),
        "the fields are kept: {page}"
    );
    Ok(())
}

#[tokio::test]
async fn a_change_is_sent_under_if_match_and_a_stale_one_is_drawn_refused()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    mount_source(&bridge).await;
    Mock::given(method("PUT"))
        .and(path(format!("/operator/sources/{ID}")))
        .and(header("if-match", "\"2\""))
        .respond_with(ResponseTemplate::new(412).set_body_json(json!({
            "type": "https://ferrobridge.example/problems#precondition-failed",
            "title": "Precondition Failed",
            "status": 412,
            "detail": "the source changed since the version If-Match names"
        })))
        .expect(1)
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, post(&format!("/sources/{ID}"), SAVE, &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(page.contains("412"), "{page}");
    Ok(())
}

#[tokio::test]
async fn a_delete_answers_303_to_the_list() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    mount_source(&bridge).await;
    Mock::given(method("DELETE"))
        .and(path(format!("/operator/sources/{ID}")))
        .and(header("if-match", "\"2\""))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, _page) = send(
        &service,
        post(
            &format!("/sources/{ID}"),
            "etag=%222%22&decision=delete",
            &cookie,
        )?,
    )
    .await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!(
        Some("/sources"),
        response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
    );
    Ok(())
}
