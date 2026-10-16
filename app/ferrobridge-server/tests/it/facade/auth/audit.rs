// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The audit line: one per FHIR request, naming who called, what was asked
//! and how it ended, and never the token or the resource content.

use std::future::Future;

use ferrobridge_server::telemetry::Rendering;
use ferrobridge_server::telemetry::subscriber;
use http::StatusCode;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::authority;
use super::bearing;
use super::ec_key;
use super::get;
use super::mint;
use super::secured;
use crate::facade::harness;
use crate::facade::raw;
use crate::support;

/// Runs `case` under a capturing JSON subscriber and returns what it logged.
fn logged<F: Future<Output = ()>>(case: F) -> String {
    let logs = support::Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    tracing::subscriber::with_default(capture, || runtime.block_on(case));
    logs.text()
}

/// Returns the audit lines of `text`, parsed.
fn audit_lines(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|line| line["target"].as_str() == Some("ferrobridge::audit"))
        .collect()
}

#[test]
fn an_admitted_request_leaves_one_audit_line_without_the_token() {
    let mut token = String::new();
    let text = logged(async {
        let key = ec_key("k1", Algorithm::ES256);
        let issuer = Issuer::start(&[&key]).await;
        let harness = harness().await;
        let app = secured(&harness, &authority(issuer.settings()).await);
        token = mint(&key, &issuer.claims("system/Condition.r"));
        let response = raw(app, bearing(get("/fhir/Condition/c-audit-1"), &token))
            .await
            .expect("an answer");
        assert_eq!(StatusCode::NOT_FOUND, response.status());
    });
    let lines = audit_lines(&text);
    assert_eq!(1, lines.len(), "{text}");
    let line = &lines[0];
    let fields = line;
    assert_eq!(
        Some("synthetic-client-sub"),
        fields["subject"].as_str(),
        "{line}"
    );
    assert_eq!(Some("synthetic-client"), fields["client"].as_str());
    assert_eq!(Some("GET"), fields["method"].as_str());
    assert_eq!(Some("read"), fields["interaction"].as_str());
    assert_eq!(Some("Condition"), fields["resource_type"].as_str());
    assert_eq!(Some("c-audit-1"), fields["resource_id"].as_str());
    assert_eq!(Some(404), fields["status"].as_u64());
    assert!(fields["duration_ms"].is_number(), "{line}");
    assert!(fields["at"].is_string(), "{line}");
    assert!(!token.is_empty());
    assert!(!text.contains(&token), "the token never reaches the log");
    let signature = token.rsplit('.').next().expect("a signature");
    assert!(!text.contains(signature), "nor its signature");
}

#[test]
fn a_refused_token_is_audited_as_unauthenticated_without_the_token() {
    let token = "synthetic-opaque-token-value";
    let text = logged(async {
        let key = ec_key("k1", Algorithm::ES256);
        let issuer = Issuer::start(&[&key]).await;
        let harness = harness().await;
        let app = secured(&harness, &authority(issuer.settings()).await);
        let response = raw(app, bearing(get("/fhir/Condition/c-audit-2"), token))
            .await
            .expect("an answer");
        assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    });
    let lines = audit_lines(&text);
    assert_eq!(1, lines.len(), "{text}");
    let fields = &lines[0];
    assert_eq!(Some("unauthenticated"), fields["subject"].as_str());
    assert_eq!(Some(401), fields["status"].as_u64());
    assert!(!text.contains(token), "the token never reaches the log");
}

#[test]
fn an_open_facade_audits_its_requests_with_no_subject() {
    let text = logged(async {
        let harness = harness().await;
        let response = raw(harness.app(), get("/fhir/Condition/c-audit-3"))
            .await
            .expect("an answer");
        assert_eq!(StatusCode::NOT_FOUND, response.status());
    });
    let lines = audit_lines(&text);
    assert_eq!(1, lines.len(), "{text}");
    assert_eq!(Some("-"), lines[0]["subject"].as_str());
}

#[test]
fn a_create_body_never_reaches_the_audit_line() {
    let text = logged(async {
        let harness = harness().await;
        let mut resource = crate::facade::condition();
        resource["note"] = serde_json::json!([{ "text": "SYNTHETIC-NOTE-CONTENT" }]);
        let response = raw(harness.app(), crate::facade::post_condition(&resource))
            .await
            .expect("an answer");
        assert!(response.status().is_client_error() || response.status().is_server_error());
    });
    let lines = audit_lines(&text);
    assert_eq!(1, lines.len(), "{text}");
    assert_eq!(Some("create"), lines[0]["interaction"].as_str());
    assert!(!text.contains("SYNTHETIC-NOTE-CONTENT"), "{text}");
}
#[test]
fn a_batch_audits_each_entry_and_the_batch_without_any_content() {
    let text = logged(async {
        let harness = harness().await;
        let mut resource = crate::facade::condition();
        resource["note"] = serde_json::json!([{ "text": "SYNTHETIC-BATCH-NOTE" }]);
        let bundle = crate::facade::batch::batch_of(&[
            crate::facade::batch::entry("GET", "Condition/c-audit-4", None),
            crate::facade::batch::entry("POST", "Condition", Some(resource)),
        ]);
        let request = http::Request::post("/fhir")
            .header(http::header::CONTENT_TYPE, "application/fhir+json")
            .body(axum::body::Body::from(bundle.to_string()))
            .expect("a legal request");
        let response = raw(harness.app(), request).await.expect("an answer");
        assert_eq!(StatusCode::OK, response.status());
    });
    let lines = audit_lines(&text);
    assert_eq!(
        3,
        lines.len(),
        "one line per entry and one for the batch: {text}"
    );
    let read = lines
        .iter()
        .find(|line| line["entry"].as_u64() == Some(0))
        .expect("the first entry's line");
    assert_eq!(Some("read"), read["interaction"].as_str());
    assert_eq!(Some("c-audit-4"), read["resource_id"].as_str());
    assert_eq!(Some(404), read["status"].as_u64());
    let create = lines
        .iter()
        .find(|line| line["entry"].as_u64() == Some(1))
        .expect("the second entry's line");
    assert_eq!(Some("create"), create["interaction"].as_str());
    let whole = lines.last().expect("the batch line");
    assert_eq!(Some("batch"), whole["interaction"].as_str(), "{text}");
    assert_eq!(Some(200), whole["status"].as_u64());
    assert!(whole["entry"].is_null(), "{text}");
    assert!(!text.contains("SYNTHETIC-BATCH-NOTE"), "{text}");
}
