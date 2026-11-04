// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping set's revisions: the Revisions tab, one revision with its
//! diff and the provisioned files read-only, and the apply posted with the
//! running revision and the reason, over a stub bridge.

use std::error::Error;

use http::StatusCode;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::stub::{answer, bridge, post, signed_in};
use crate::support::{OPERATOR_TOKEN, get_with, send};

/// The wire path of the console file the revisions hold.
const FILE: &str = "fhirconnect/synthetic_console.yml";

/// Mounts the history, revision 2 and its diff against the running 3.
async fn history(bridge: &MockServer) {
    answer(
        bridge,
        "/operator/revisions",
        200,
        &json!({"running": "3", "revisions": [
            {"id": "3", "parent": "2", "author": "operator-1", "time": "2026-11-03T09:05:00Z",
             "reason": "synthetic second save", "origin": "save", "applied": true, "files": 1},
            {"id": "2", "parent": "1", "author": "operator-1", "time": "2026-11-03T09:00:00Z",
             "reason": "synthetic first save", "origin": "save", "applied": true, "files": 1},
            {"id": "1", "parent": null, "author": "-", "time": "2026-11-03T08:00:00Z",
             "reason": "the console files changed on disk", "origin": "baseline", "applied": true, "files": 0}
        ]}),
    )
    .await;
    answer(
        bridge,
        "/operator/revisions/2",
        200,
        &json!({"id": "2", "parent": "1", "author": "operator-1", "time": "2026-11-03T09:00:00Z",
                "reason": "synthetic first save", "origin": "save", "applied": true, "source": null,
                "files": {FILE: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}),
    )
    .await;
    answer(
        bridge,
        "/operator/revisions/2/diff",
        200,
        &json!({"from": "3", "to": "2", "files": [{
            "path": FILE, "change": "changed",
            "from_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "to_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "unified": "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-name: synthetic_new\n+name: synthetic_old\n"
        }]}),
    )
    .await;
}

#[tokio::test]
async fn the_revisions_tab_lists_each_revision_with_its_standing() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    history(&bridge).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, get_with("/mappings/revisions", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    for words in [
        "/mappings/revisions/2",
        "synthetic second save",
        "2026-11-03T09:05:00Z",
        "Running",
        "Ran before",
        "Baseline",
    ] {
        assert!(page.contains(words), "{words}: {page}");
    }
    Ok(())
}

#[tokio::test]
async fn a_bridge_that_saves_nothing_keeps_no_revisions() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    answer(
        &bridge,
        "/operator/revisions",
        409,
        &json!({"type": "https://ferrobridge.eu/docs/operate/problems.html#not-configured", "title": "No mapping directory is configured", "status": 409}),
    )
    .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/mappings/revisions", &cookie)?).await?;
    assert!(page.contains("keeps no revisions"), "{page}");
    Ok(())
}

#[tokio::test]
async fn a_revision_shows_its_diff_the_provisioned_files_and_the_apply()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    history(&bridge).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, get_with("/mappings/revisions/2", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    for words in [
        "Revision 2",
        "synthetic first save",
        FILE,
        r#"<span class="fui-sr-only">removed </span>name: synthetic_new"#,
        r#"<span class="fui-sr-only">added </span>name: synthetic_old"#,
        "Provisioned files",
        "Roll back to this revision",
        r#"name="running" value="3""#,
        r#"action="/mappings/revisions/2""#,
    ] {
        assert!(page.contains(words), "{words}: {page}");
    }
    Ok(())
}

#[tokio::test]
async fn the_running_revision_offers_no_apply() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    history(&bridge).await;
    answer(
        &bridge,
        "/operator/revisions/3",
        200,
        &json!({"id": "3", "parent": "2", "author": "operator-1", "time": "2026-11-03T09:05:00Z",
                "reason": "synthetic second save", "origin": "save", "applied": true, "source": null,
                "files": {FILE: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}),
    )
    .await;
    answer(
        &bridge,
        "/operator/revisions/3/diff",
        200,
        &json!({"from": "3", "to": "3", "files": []}),
    )
    .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/mappings/revisions/3", &cookie)?).await?;
    assert!(page.contains("are the files that run"), "{page}");
    assert!(!page.contains("Roll back to this revision"), "{page}");
    Ok(())
}

#[tokio::test]
async fn an_apply_posts_the_running_revision_and_the_reason_and_draws_the_answer()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    history(&bridge).await;
    Mock::given(method("POST"))
        .and(path("/operator/revisions/2/apply"))
        .and(header("if-match", "\"3\""))
        .and(header("ferrobridge-reason", "synthetic rollback"))
        .and(header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"4\"")
                .set_body_json(json!({"clean": true, "counts": {"fhirconnect_files": 1, "contexts": 1, "programs": 1, "templates": 1, "omocl_files": 0}, "diagnostics": []})),
        )
        .expect(1)
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(
        &service,
        post(
            "/mappings/revisions/2",
            "running=3&reason=synthetic+rollback&decision=apply",
            &cookie,
        )?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(page.contains("runs as revision 4"), "{page}");
    Ok(())
}

#[tokio::test]
async fn an_apply_without_a_reason_is_refused_before_the_bridge_is_asked()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    history(&bridge).await;
    Mock::given(method("POST"))
        .and(path("/operator/revisions/2/apply"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(
        &service,
        post(
            "/mappings/revisions/2",
            "running=3&reason=&decision=apply",
            &cookie,
        )?,
    )
    .await?;
    assert!(page.contains("Give a reason to save."), "{page}");
    Ok(())
}

#[tokio::test]
async fn a_stale_apply_draws_the_bridges_refusal() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    history(&bridge).await;
    Mock::given(method("POST"))
        .and(path("/operator/revisions/2/apply"))
        .respond_with(ResponseTemplate::new(412).set_body_json(json!({
            "type": "https://ferrobridge.eu/docs/operate/problems.html#precondition-failed",
            "title": "What the request replaces changed since the ETag it names",
            "status": 412
        })))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(
        &service,
        post(
            "/mappings/revisions/2",
            "running=2&reason=r&decision=apply",
            &cookie,
        )?,
    )
    .await?;
    assert!(page.contains("412"), "{page}");
    Ok(())
}
