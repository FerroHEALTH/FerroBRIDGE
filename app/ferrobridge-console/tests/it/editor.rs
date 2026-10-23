// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The editor against a stub bridge: the file opened with its digest in a
//! plain `POST` form, a check of the draft over the running set, the save
//! confirmed in the bar and sent under `If-Match` with its reason and the
//! draft's exact bytes, a new file under `If-None-Match`, the refused draft
//! that leaves the set unchanged, the `412` conflict, the reason checked
//! before anything is sent, and no draft in any log line.

use std::error::Error;

use http::StatusCode;
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::stub::{CONTEXT, DIGEST, MODEL, bridge, post, signed_in};
use crate::support::{Captured, get_with, send};

/// The draft the operator saves, with a marker no log may carry.
const DRAFT: &str = "grammar: FHIRConnect/v1.0.0\ntype: context\nmetadata:\n  name: synthetic-draft-marker\n# naïve <b>\n";

/// The editor of the stub's console context.
fn editor() -> String {
    format!("/mappings/edit/{CONTEXT}")
}

/// The form of a post of [`DRAFT`] deciding `decision` with `reason`.
fn form(decision: &str, reason: &str, base: &str, path: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("draft", DRAFT),
            ("base", base),
            ("path", path),
            ("reason", reason),
            ("decision", decision),
        ])
        .finish()
}

/// The report of a set that compiled.
fn clean() -> serde_json::Value {
    json!({"clean": true, "counts": {"fhirconnect_files": 1, "contexts": 1, "programs": 1, "templates": 1, "omocl_files": 0}, "diagnostics": []})
}

/// The report of a set the draft broke.
fn broken() -> serde_json::Value {
    json!({"clean": false, "counts": {"fhirconnect_files": 1, "contexts": 1, "programs": 1, "templates": 1, "omocl_files": 0}, "diagnostics": [{
        "file": CONTEXT, "line": 4, "column": 9, "severity": "error",
        "code": "invalid-mapping-name", "message": "the synthetic name is refused",
        "mapping": null, "model_path": ["metadata", "name"]
    }]})
}

/// The requests of `method` the stub bridge received.
async fn received(bridge: &MockServer, verb: &str) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
    let asked = bridge.received_requests().await.ok_or("recorded")?;
    Ok(asked
        .into_iter()
        .filter(|request| request.method.as_str() == verb)
        .map(|request| request.body)
        .collect())
}

#[tokio::test]
async fn the_editor_opens_a_console_file_in_a_post_form_with_its_digest_and_the_save()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, body) = send(&service, get_with(&editor(), &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        body.contains(r#"<form id="editor" method="post""#),
        "{body}"
    );
    assert!(
        body.contains(&format!(r#"name="base" value="{DIGEST}""#)),
        "{body}"
    );
    assert!(body.contains("name=\"draft\""), "{body}");
    assert!(body.contains("synthetic-observation"), "{body}");
    assert!(body.contains("Save and reload"), "{body}");
    assert!(body.contains("Not checked yet"), "{body}");
    Ok(())
}

#[tokio::test]
async fn the_editor_says_new_when_it_creates_and_check_a_draft_for_a_provisioned_file()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("GET"))
        .and(path(format!("/operator/mappings/{MODEL}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"abc\"")
                .set_body_raw("grammar: FHIRConnect/v1.0.0\n", "application/yaml"),
        )
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, new) = send(&service, get_with("/mappings/new", &cookie)?).await?;
    assert!(new.contains(">New mapping file</h1>"), "{new}");
    assert!(new.contains("Create and reload"), "{new}");
    assert!(new.contains(r#"name="path""#), "{new}");
    let (_response, provisioned) = send(
        &service,
        get_with(&format!("/mappings/edit/{MODEL}"), &cookie)?,
    )
    .await?;
    assert!(provisioned.contains(">Check a draft</h1>"), "{provisioned}");
    assert!(
        provisioned.contains("Save under the new path"),
        "{provisioned}"
    );
    assert!(
        provisioned.contains("so the console never saves over it"),
        "{provisioned}"
    );
    Ok(())
}

#[tokio::test]
async fn a_check_sends_the_draft_verbatim_and_draws_its_diagnostics() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/operator/mappings/check"))
        .and(query_param("path", CONTEXT))
        .respond_with(ResponseTemplate::new(200).set_body_json(broken()))
        .expect(1)
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, body) = send(
        &service,
        post(&editor(), &form("check", "", DIGEST, ""), &cookie)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("invalid-mapping-name"), "{body}");
    assert!(
        body.contains("The set does not compile with this draft: 1 error."),
        "{body}"
    );
    assert!(body.contains("synthetic-draft-marker"), "the draft stays");
    assert_eq!(
        vec![DRAFT.as_bytes().to_vec()],
        received(&bridge, "POST").await?
    );
    Ok(())
}

#[tokio::test]
async fn a_save_is_confirmed_then_sent_byte_identical_under_if_match_with_its_reason()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("PUT"))
        .and(path(format!("/operator/mappings/{CONTEXT}")))
        .and(header("if-match", format!("\"{DIGEST}\"").as_str()))
        .and(header("ferrobridge-reason", "synthetic reason"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"def0def0def0def0def0\"")
                .set_body_json(clean()),
        )
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, asked) = send(
        &service,
        post(
            &editor(),
            &form("save", "synthetic reason", DIGEST, ""),
            &cookie,
        )?,
    )
    .await?;
    assert!(asked.contains("Save and reload the set?"), "{asked}");
    assert!(asked.contains("Confirm the save"), "{asked}");
    assert!(
        received(&bridge, "PUT").await?.is_empty(),
        "nothing sent yet"
    );
    let (_response, saved) = send(
        &service,
        post(
            &editor(),
            &form("confirm", "synthetic reason", DIGEST, ""),
            &cookie,
        )?,
    )
    .await?;
    assert!(
        saved.contains("Saved, and the set reloaded with it."),
        "{saved}"
    );
    assert!(saved.contains("def0def0"), "the new digest: {saved}");
    assert_eq!(
        vec![DRAFT.as_bytes().to_vec()],
        received(&bridge, "PUT").await?
    );
    Ok(())
}

#[tokio::test]
async fn a_new_file_is_saved_under_its_path_with_if_none_match() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let named = "fhirconnect/models/synthetic-new.model.yml";
    Mock::given(method("PUT"))
        .and(path(format!("/operator/mappings/{named}")))
        .and(header("if-none-match", "*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(clean()))
        .expect(1)
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, body) = send(
        &service,
        post(
            "/mappings/new",
            &form("confirm", "a new synthetic file", "", named),
            &cookie,
        )?,
    )
    .await?;
    assert!(
        body.contains("Saved, and the set reloaded with it."),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_path_that_climbs_is_refused_at_its_field_before_the_bridge_is_asked()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, body) = send(
        &service,
        post(
            "/mappings/new",
            &form(
                "confirm",
                "a synthetic reason",
                "",
                "fhirconnect/../escape.yml",
            ),
            &cookie,
        )?,
    )
    .await?;
    assert!(body.contains("Remove the empty, . or .. step"), "{body}");
    assert_eq!(0, received(&bridge, "PUT").await?.len());
    Ok(())
}

#[tokio::test]
async fn a_refused_draft_writes_nothing_and_leaves_the_set_unchanged() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    Mock::given(method("PUT"))
        .and(path(format!("/operator/mappings/{CONTEXT}")))
        .respond_with(ResponseTemplate::new(422).set_body_json(broken()))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, body) = send(
        &service,
        post(
            &editor(),
            &form("confirm", "synthetic reason", DIGEST, ""),
            &cookie,
        )?,
    )
    .await?;
    assert!(
        body.contains("nothing was written and the running set is unchanged"),
        "{body}"
    );
    assert!(body.contains("invalid-mapping-name"), "{body}");
    let asked = bridge.received_requests().await.ok_or("recorded")?;
    assert!(
        !asked
            .iter()
            .any(|request| request.url.path() == "/operator/reload"),
        "no reload follows a refused draft"
    );
    Ok(())
}

#[tokio::test]
async fn a_file_that_changed_meanwhile_is_a_conflict_never_overwritten()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("PUT"))
        .and(path(format!("/operator/mappings/{CONTEXT}")))
        .respond_with(
            ResponseTemplate::new(412)
                .set_body_json(json!({"resourceType": "OperationOutcome", "issue": []})),
        )
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, body) = send(
        &service,
        post(
            &editor(),
            &form("confirm", "synthetic reason", "stale", ""),
            &cookie,
        )?,
    )
    .await?;
    assert!(
        body.contains("The file changed since you opened it, so nothing was written."),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_save_without_a_reason_is_refused_at_its_field_and_never_sent()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for decision in ["save", "confirm"] {
        let (_response, body) = send(
            &service,
            post(&editor(), &form(decision, "  ", DIGEST, ""), &cookie)?,
        )
        .await?;
        assert!(
            body.contains("Give a reason to save."),
            "{decision}: {body}"
        );
    }
    let (_response, body) = send(
        &service,
        post(
            &editor(),
            &form("confirm", "tick \u{2713}", DIGEST, ""),
            &cookie,
        )?,
    )
    .await?;
    assert!(
        body.contains("Use plain letters, digits and punctuation."),
        "{body}"
    );
    assert_eq!(0, received(&bridge, "PUT").await?.len());
    Ok(())
}

#[tokio::test]
async fn no_draft_reaches_a_log_line() -> Result<(), Box<dyn Error>> {
    let captured = Captured::default();
    let _guard = captured.logging();
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/operator/mappings/check"))
        .respond_with(ResponseTemplate::new(200).set_body_json(clean()))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, _body) = send(
        &service,
        post(&editor(), &form("check", "", DIGEST, ""), &cookie)?,
    )
    .await?;
    assert!(
        !captured.text().contains("synthetic-draft-marker"),
        "{}",
        captured.text()
    );
    Ok(())
}

#[tokio::test]
async fn a_draft_posted_with_the_crlf_a_text_area_submits_is_saved_with_lf()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("PUT"))
        .and(path(format!("/operator/mappings/{CONTEXT}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(clean()))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let submitted = DRAFT.replace('\n', "\r\n");
    let posted = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("draft", submitted.as_str()),
            ("base", DIGEST),
            ("path", ""),
            ("reason", "synthetic reason"),
            ("decision", "confirm"),
        ])
        .finish();
    let (_response, saved) = send(&service, post(&editor(), &posted, &cookie)?).await?;
    assert!(
        saved.contains("Saved, and the set reloaded with it."),
        "{saved}"
    );
    assert_eq!(
        vec![DRAFT.as_bytes().to_vec()],
        received(&bridge, "PUT").await?
    );
    Ok(())
}
