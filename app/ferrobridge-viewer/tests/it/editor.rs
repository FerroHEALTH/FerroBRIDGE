// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping file editor against a stub bridge: the file opened with its
//! digest, a draft checked over the running set, a save under `If-Match`
//! with its reason and the draft's exact bytes, the refused draft that
//! leaves the set unchanged, the `412` conflict, the confirmation the server
//! checks again, the reload, and no draft in any log line.

use std::error::Error;

use axum::body::Body;
use ferrobridge_testkit::oidc::{Provider, TokenAnswer};
use http::{Request, StatusCode};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{Captured, OPERATOR_TOKEN, configuration, console, get_with, send, sign_in};

/// The console file the editor opens.
const PATH: &str = "fhirconnect/models/synthetic-console.model.yml";

/// Its text, with a multi-byte character and characters HTML escapes.
const TEXT: &str = "grammar: FHIRConnect/v1.0.0\ntype: model\n# naïve <b>\n";

/// The draft the operator saves, with a marker no log may carry.
const DRAFT: &str =
    "grammar: FHIRConnect/v1.0.0\ntype: model\nmetadata:\n  name: synthetic-draft-marker\n";

/// The report of a set that compiled.
fn clean() -> serde_json::Value {
    json!({"clean": true, "counts": {"fhirconnect_files": 1, "contexts": 1, "programs": 1, "templates": 1, "omocl_files": 0}, "diagnostics": []})
}

/// The report of a set the draft broke.
fn broken() -> serde_json::Value {
    json!({"clean": false, "counts": {"fhirconnect_files": 1, "contexts": 1, "programs": 1, "templates": 1, "omocl_files": 0}, "diagnostics": [{
        "file": PATH, "line": 4, "column": 9, "severity": "error",
        "code": "invalid-mapping-name", "message": "the synthetic name is refused",
        "mapping": null, "model_path": ["metadata", "name"]
    }]})
}

/// A stub bridge serving the file `PATH`, owned by `owner`.
async fn bridge(owner: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/operator/mappings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "path": PATH, "language": "fhirconnect", "kind": "model", "sha256": "abc",
            "owner": owner, "read_only": owner == "provisioned", "diagnostics": []
        }])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/operator/mappings/{PATH}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"abc\"")
                .set_body_raw(TEXT, "application/yaml"),
        )
        .mount(&server)
        .await;
    server
}

/// The console over `bridge` and a signed-in session's cookie.
async fn signed_in(bridge: &MockServer) -> Result<(axum::Router, String), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let base = format!("{}/", bridge.uri());
    let (_state, service) = console(&configuration(&provider, &base, "", ""))?;
    let cookie = sign_in(&service, &provider, TokenAnswer::bearer(OPERATOR_TOKEN)).await?;
    Ok((service, cookie))
}

/// What the server function at `route` answers `pairs` with, as JSON.
async fn called(
    service: &axum::Router,
    route: &str,
    pairs: &[(&str, &str)],
    cookie: &str,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish();
    let request = Request::post(route)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", cookie)
        .body(Body::from(body))?;
    let (response, body) = send(service, request).await?;
    assert_eq!(StatusCode::OK, response.status(), "{route}: {body}");
    Ok(serde_json::from_str(&body)?)
}

/// Saves [`DRAFT`] read at `etag` with `reason`, `confirmed` as given.
async fn save(
    service: &axum::Router,
    cookie: &str,
    etag: &str,
    reason: &str,
    confirmed: &str,
) -> Result<serde_json::Value, Box<dyn Error>> {
    called(
        service,
        "/api/save",
        &[
            ("path", PATH),
            ("draft", DRAFT),
            ("etag", etag),
            ("reason", reason),
            ("confirmed", confirmed),
        ],
        cookie,
    )
    .await
}

/// The `html` of a save's answer.
fn html(answer: &serde_json::Value) -> &str {
    answer["html"].as_str().unwrap_or_default()
}

#[tokio::test]
async fn the_editor_opens_a_console_file_with_its_text_and_the_save() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge("console").await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, get_with(&format!("/edit/{PATH}"), &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        !page.contains("# naïve <b>"),
        "the text reaches the editor once hydrated, never as markup: {page}"
    );
    assert!(page.contains(r#"action="/api/save""#), "{page}");
    assert!(page.contains(r#"<label for="edit-confirm">"#), "{page}");
    let source = called(&service, "/api/source", &[("path", PATH)], &cookie).await?;
    assert_eq!(json!(TEXT), source["Shown"]["text"]);
    assert_eq!(json!("abc"), source["Shown"]["etag"]);
    assert_eq!(json!(true), source["Shown"]["writable"]);
    Ok(())
}

#[tokio::test]
async fn a_provisioned_file_opens_for_a_check_and_offers_no_save() -> Result<(), Box<dyn Error>> {
    let bridge = bridge("provisioned").await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with(&format!("/edit/{PATH}"), &cookie)?).await?;
    assert!(page.contains("never saves over it"), "{page}");
    assert!(!page.contains(r#"action="/api/save""#), "{page}");
    assert!(page.contains(r#"action="/api/check""#), "{page}");
    Ok(())
}

#[tokio::test]
async fn a_check_sends_the_draft_verbatim_and_draws_its_diagnostics() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge("console").await;
    Mock::given(method("POST"))
        .and(path("/operator/mappings/check"))
        .and(query_param("path", PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(broken()))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let answer = called(
        &service,
        "/api/check",
        &[("path", PATH), ("draft", DRAFT)],
        &cookie,
    )
    .await?;
    let html = answer.as_str().unwrap_or_default();
    assert!(html.contains("does not compile with this draft"), "{html}");
    assert!(html.contains("the synthetic name is refused"), "{html}");
    let asked = bridge.received_requests().await.ok_or("recorded")?;
    let check = asked
        .iter()
        .find(|request| request.url.path() == "/operator/mappings/check")
        .ok_or("a check")?;
    assert_eq!(
        DRAFT.as_bytes(),
        check.body.as_slice(),
        "the draft verbatim"
    );
    Ok(())
}

#[tokio::test]
async fn a_save_is_byte_identical_under_if_match_with_its_reason_and_reloads()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge("console").await;
    Mock::given(method("PUT"))
        .and(path(format!("/operator/mappings/{PATH}")))
        .and(header("if-match", "\"abc\""))
        .and(header("ferrobridge-reason", "synthetic reason"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"def\"")
                .set_body_json(clean()),
        )
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let answer = save(&service, &cookie, "abc", "synthetic reason", "yes").await?;
    assert!(
        html(&answer).contains("Saved, and the set reloaded with it."),
        "{answer}"
    );
    assert!(html(&answer).contains("<code>def</code>"), "{answer}");
    assert_eq!(
        json!("def"),
        answer["etag"],
        "the next save is under the new digest"
    );
    let asked = bridge.received_requests().await.ok_or("recorded")?;
    let put = asked
        .iter()
        .find(|request| request.method.as_str() == "PUT")
        .ok_or("a save")?;
    assert_eq!(DRAFT.as_bytes(), put.body.as_slice(), "byte-identical");
    Ok(())
}

#[tokio::test]
async fn a_new_file_is_saved_under_if_none_match() -> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(format!("/operator/mappings/{PATH}")))
        .and(header("if-none-match", "*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(clean()))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let answer = save(&service, &cookie, "", "a new synthetic file", "yes").await?;
    assert!(html(&answer).contains("Saved"), "{answer}");
    Ok(())
}

#[tokio::test]
async fn a_refused_draft_writes_nothing_and_leaves_the_set_unchanged() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge("console").await;
    Mock::given(method("PUT"))
        .and(path(format!("/operator/mappings/{PATH}")))
        .respond_with(ResponseTemplate::new(422).set_body_json(broken()))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let answer = save(&service, &cookie, "abc", "synthetic reason", "yes").await?;
    assert!(
        html(&answer).contains("nothing was written and the running set is unchanged"),
        "{answer}"
    );
    assert!(html(&answer).contains("invalid-mapping-name"), "{answer}");
    assert_eq!(json!(null), answer["etag"]);
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
    let bridge = bridge("console").await;
    Mock::given(method("PUT"))
        .and(path(format!("/operator/mappings/{PATH}")))
        .respond_with(
            ResponseTemplate::new(412)
                .set_body_json(json!({"resourceType": "OperationOutcome", "issue": []})),
        )
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let answer = save(&service, &cookie, "stale", "synthetic reason", "yes").await?;
    assert!(
        html(&answer).contains("412: the file changed since you opened it"),
        "{answer}"
    );
    Ok(())
}

#[tokio::test]
async fn a_save_needs_its_confirmation_and_a_reason_before_it_is_sent() -> Result<(), Box<dyn Error>>
{
    let bridge = MockServer::start().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let unconfirmed = save(&service, &cookie, "abc", "synthetic reason", "no").await?;
    assert!(
        html(&unconfirmed).contains("Confirm the save"),
        "{unconfirmed}"
    );
    for reason in ["", "two\nlines", "naïve", &"x".repeat(201)] {
        let answer = save(&service, &cookie, "abc", reason, "yes").await?;
        assert!(
            html(&answer).contains("printable ASCII"),
            "{reason:?}: {answer}"
        );
    }
    let asked = bridge.received_requests().await.ok_or("recorded")?;
    assert!(
        asked.is_empty(),
        "the bridge was asked {} times",
        asked.len()
    );
    Ok(())
}

#[tokio::test]
async fn a_reload_draws_whether_the_set_was_replaced() -> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/operator/reload"))
        .respond_with(ResponseTemplate::new(422).set_body_json(broken()))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let answer = called(&service, "/api/reload", &[], &cookie).await?;
    let html = answer.as_str().unwrap_or_default();
    assert!(html.contains("the running set is unchanged"), "{html}");
    Ok(())
}

#[tokio::test]
async fn no_draft_reaches_a_log_line() -> Result<(), Box<dyn Error>> {
    let captured = Captured::default();
    let _guard = captured.logging();
    let bridge = bridge("console").await;
    Mock::given(method("PUT"))
        .and(path(format!("/operator/mappings/{PATH}")))
        .respond_with(
            ResponseTemplate::new(409)
                .set_body_json(json!({"resourceType": "OperationOutcome", "issue": []})),
        )
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    save(&service, &cookie, "abc", "synthetic reason", "yes").await?;
    called(
        &service,
        "/api/check",
        &[("path", PATH), ("draft", DRAFT)],
        &cookie,
    )
    .await?;
    let logged = captured.text();
    assert!(
        !logged.contains("synthetic-draft-marker"),
        "a log line carries the draft: {logged}"
    );
    Ok(())
}
