// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A stub bridge with synthetic documents for every surface the console
//! reads, each answered for the operator's own token alone, and a console
//! signed in against it.

use std::error::Error;

use axum::body::Body;
use ferrobridge_testkit::oidc::{Provider, TokenAnswer};
use http::Request;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{OPERATOR_TOKEN, configuration, console, sign_in};

/// The wire path of the stub's console context file.
pub(crate) const CONTEXT: &str = "fhirconnect/contexts/observation.context.yml";

/// The wire path of the stub's provisioned model file.
pub(crate) const MODEL: &str = "fhirconnect/models/bp.model.yml";

/// The wire path of the stub's file with an error.
pub(crate) const BROKEN: &str = "fhirconnect/contexts/broken.context.yml";

/// The wire path of the stub's OMOCL file.
pub(crate) const OMOCL: &str = "omocl/measurement.yml";

/// The text of the stub's context file.
pub(crate) const CONTEXT_TEXT: &str =
    "grammar: FHIRConnect/v1.0.0\ntype: context\nmetadata:\n  name: synthetic-observation\n";

/// The digest the stub serves the context file under.
pub(crate) const DIGEST: &str = "5712e3f7aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaf2e0e3fa";

/// The synthetic `GET /health/info` document.
pub(crate) fn info() -> serde_json::Value {
    json!({
        "product": "FerroBRIDGE",
        "version": "0.0.8",
        "build": {"commit": "0123456789abcdef", "commit_short": "0123456789ab", "built_at": "2026-10-17T00:00:00Z", "rustc": "rustc 1.98.1"},
        "pins": [{"name": "FHIRconnect", "version": "1.0.0"}, {"name": "OMOP CDM", "version": "5.4"}],
        "lanes": [
            {"name": "facade", "enabled": true},
            {"name": "hl7v2", "enabled": true, "listen": "0.0.0.0:2575", "up": true},
            {"name": "operations", "enabled": true},
            {"name": "etl", "enabled": true},
            {"name": "terminology", "enabled": true}
        ]
    })
}

/// The synthetic `GET /health/readiness` document: every indicator up but
/// the terminology server.
pub(crate) fn readiness() -> serde_json::Value {
    json!({
        "state": "down",
        "indicators": {
            "cdr": {"state": "up"},
            "terminology": {"state": "down", "detail": "answers 503"},
            "cdm": {"state": "up"},
            "identity-store": {"state": "up"},
            "hl7v2-listener": {"state": "up"}
        }
    })
}

/// The synthetic `GET /operator/state` document: one context, two
/// templates, one OMOCL file and one error.
pub(crate) fn state() -> serde_json::Value {
    json!({
        "contexts": [{
            "context": "synthetic-observation",
            "file": CONTEXT,
            "template": "synthetic.vital_signs.v1",
            "profile": "http://example.org/fhir/StructureDefinition/synthetic-observation",
            "profile_version": "1.0.0",
            "resource_type": "Observation",
            "models": [{
                "name": "synthetic-blood-pressure",
                "file": MODEL,
                "extensions": []
            }],
            "search": null
        }],
        "templates": [
            {"id": "synthetic.vital_signs.v1", "source": "directory", "file": "templates/vital_signs.opt"},
            {"id": "synthetic.lab.v1", "source": "cdr"}
        ],
        "omocl": {"files": [OMOCL], "valid": true},
        "counts": {"fhirconnect_files": 3, "contexts": 1, "programs": 1, "templates": 2, "omocl_files": 1},
        "diagnostics": [{
            "file": BROKEN,
            "line": 4, "column": 3,
            "severity": "error", "code": "missing-key",
            "message": "the synthetic <script>alert(1)</script> key is missing",
            "mapping": null, "model_path": ["spec"]
        }]
    })
}

/// The synthetic `GET /operator/mappings` listing.
pub(crate) fn listing() -> serde_json::Value {
    json!([
        {"path": CONTEXT, "language": "fhirconnect", "kind": "context", "sha256": DIGEST, "owner": "console", "read_only": false, "diagnostics": []},
        {"path": MODEL, "language": "fhirconnect", "kind": "model", "sha256": DIGEST, "owner": "provisioned", "read_only": true, "diagnostics": []},
        {"path": BROKEN, "language": "fhirconnect", "kind": null, "sha256": DIGEST, "owner": "provisioned", "read_only": true, "diagnostics": [{
            "file": BROKEN, "line": 4, "column": 3, "severity": "error", "code": "missing-key",
            "message": "the synthetic key is missing", "mapping": null, "model_path": ["spec"]
        }]},
        {"path": OMOCL, "language": "omocl", "kind": "mapping", "sha256": DIGEST, "owner": "console", "read_only": false, "diagnostics": []}
    ])
}

/// A synthetic R4 `CapabilityStatement` with one resource.
pub(crate) fn statement() -> serde_json::Value {
    json!({
        "resourceType": "CapabilityStatement",
        "status": "active",
        "date": "2026-10-17",
        "kind": "instance",
        "software": {"name": "FerroBRIDGE", "version": "0.0.8"},
        "fhirVersion": "4.0.1",
        "format": ["json"],
        "rest": [{
            "mode": "server",
            "resource": [{
                "type": "Observation",
                "supportedProfile": ["http://example.org/fhir/StructureDefinition/synthetic-observation"],
                "interaction": [{"code": "read"}, {"code": "search-type"}],
                "searchParam": [{"name": "code", "type": "token"}],
                "operation": [{"name": "validate", "definition": "http://hl7.org/fhir/OperationDefinition/Resource-validate"}]
            }]
        }]
    })
}

/// One entry of the synthetic running configuration.
fn entry(key: &str, value: &serde_json::Value, source: &str) -> serde_json::Value {
    json!({"key": key, "value": value, "source": source})
}

/// The synthetic `GET /operator/configuration` document: each lane's keys,
/// each secret by reference alone.
pub(crate) fn running_configuration() -> serde_json::Value {
    let text = |text: &str| json!({"kind": "string", "value": text});
    let number = |number: i64| json!({"kind": "integer", "value": number});
    let switch = |on: bool| json!({"kind": "boolean", "value": on});
    json!({
        "file": "/etc/ferrobridge/ferrobridge.toml",
        "read_at": "2026-10-24T06:00:00Z",
        "entries": [
            entry("auth.audience", &json!({"kind": "list", "value": [text("https://bridge.example.org/fhir")]}), "file"),
            entry("auth.issuer", &text("https://idp.example.org/realms/bridge"), "file"),
            entry("auth.product_scope", &text("__ferrobridge.operator"), "default"),
            entry("cdm.bridge_schema", &text("ferrobridge"), "default"),
            entry("cdm.person_policy", &text("create_on_first_sight"), "default"),
            entry("cdm.schema", &text("cdm"), "default"),
            entry("cdm.url", &json!({"kind": "secret", "value": {"state": "file", "path": "/run/secrets/cdm-url"}}), "file"),
            entry("cdr.base_url", &text("https://cdr.example.org/openehr/v1"), "file"),
            entry("cdr.credentials.oauth.client_id", &text("ferrobridge"), "file"),
            entry("cdr.credentials.oauth.client_secret", &json!({"kind": "secret", "value": {"state": "file", "path": "/run/secrets/cdr.secret"}}), "file"),
            entry("cdr.credentials.oauth.token_endpoint", &text("https://idp.example.org/realms/bridge/protocol/openid-connect/token"), "file"),
            entry("cdr.credentials.password", &json!({"kind": "secret", "value": {"state": "unset"}}), "default"),
            entry("cdr.demographic", &switch(false), "default"),
            entry("cdr.retry.initial_backoff_ms", &number(200), "default"),
            entry("cdr.retry.max_attempts", &number(3), "default"),
            entry("cdr.retry.max_backoff_ms", &number(5000), "default"),
            entry("cdr.timeout_ms", &number(30_000), "default"),
            entry("etl.aql", &text("SELECT e/ehr_id/value AS ehr_id FROM EHR e"), "file"),
            entry("etl.deletions", &text("remove"), "default"),
            entry("etl.page_size", &number(100), "default"),
            entry("etl.vocabulary_aliases", &json!({"kind": "table", "value": {}}), "default"),
            entry("facade.base_url", &text("https://bridge.example.org/fhir"), "file"),
            entry("facade.composition_language", &text("en"), "file"),
            entry("facade.composition_territory", &text("NL"), "file"),
            entry("facade.ehr_policy", &text("existing"), "default"),
            entry("facade.enabled", &switch(true), "file"),
            entry("facade.identifier_search.enabled", &switch(false), "default"),
            entry("facade.identity_store", &text("identity.redb"), "default"),
            entry("facade.references.fetch_bases", &json!({"kind": "list", "value": []}), "default"),
            entry("facade.references.max_depth", &number(2), "default"),
            entry("facade.search_count", &number(20), "default"),
            entry("facade.search_count_max", &number(100), "default"),
            entry("hl7v2.default_charset", &text("ASCII"), "default"),
            entry("hl7v2.enabled", &switch(true), "file"),
            entry("hl7v2.frame_limit_bytes", &number(1_048_576), "default"),
            entry("hl7v2.frame_timeout_ms", &number(30_000), "default"),
            entry("hl7v2.idle_timeout_ms", &number(300_000), "default"),
            entry("hl7v2.listen", &text("0.0.0.0:2575"), "environment"),
            entry("hl7v2.senders", &json!({"kind": "list", "value": []}), "default"),
            entry("hl7v2.unmapped_entries", &text("skip_and_count"), "default"),
            entry("mappings.directory", &text("/etc/ferrobridge/mappings"), "file"),
            entry("operations.enabled", &switch(true), "default"),
            entry("operator.enabled", &switch(true), "file"),
            entry("server.body_limit_bytes", &number(1_048_576), "default"),
            entry("server.listen", &text("0.0.0.0:8080"), "file"),
            entry("server.request_timeout_ms", &number(30_000), "default"),
            entry("telemetry.filter", &text("info"), "default"),
            entry("telemetry.format", &text("json"), "environment"),
            entry("telemetry.logged_query_parameters", &json!({"kind": "list", "value": []}), "default"),
            entry("terminology.base_url", &text("https://term.example.org/fhir"), "file"),
            entry("terminology.credentials.password", &json!({"kind": "secret", "value": {"state": "set"}}), "environment"),
            entry("terminology.credentials.user", &text("bridge"), "file"),
            entry("terminology.display_fallback", &switch(false), "default"),
            entry("terminology.wire_version", &text("r4"), "default"),
            entry("usage_report.enabled", &switch(true), "default")
        ]
    })
}

/// The synthetic SMART discovery document.
pub(crate) fn smart() -> serde_json::Value {
    json!({
        "issuer": "https://idp.example.org/realms/bridge",
        "capabilities": ["permission-v2", "launch-ehr"]
    })
}

/// Mounts `body` with `status` at `GET route` on `bridge`, for the
/// operator's token alone.
pub(crate) async fn answer(
    bridge: &MockServer,
    route: &str,
    status: u16,
    body: &serde_json::Value,
) {
    Mock::given(method("GET"))
        .and(path(route))
        .and(header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(bridge)
        .await;
}

/// A stub bridge that answers every document the console reads.
pub(crate) async fn bridge() -> MockServer {
    let server = MockServer::start().await;
    answer(&server, "/health/info", 200, &info()).await;
    answer(&server, "/health/readiness", 503, &readiness()).await;
    answer(&server, "/operator/state", 200, &state()).await;
    answer(&server, "/operator/mappings", 200, &listing()).await;
    answer(
        &server,
        "/operator/configuration",
        200,
        &running_configuration(),
    )
    .await;
    answer(&server, "/fhir/metadata", 200, &statement()).await;
    answer(
        &server,
        "/fhir/.well-known/smart-configuration",
        200,
        &smart(),
    )
    .await;
    Mock::given(method("GET"))
        .and(path(format!("/operator/mappings/{CONTEXT}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", format!("\"{DIGEST}\"").as_str())
                .set_body_raw(CONTEXT_TEXT, "application/yaml"),
        )
        .mount(&server)
        .await;
    server
}

/// The console over `bridge` and a signed-in session's cookie.
pub(crate) async fn signed_in(
    bridge: &MockServer,
) -> Result<(axum::Router, String), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let base = format!("{}/", bridge.uri());
    let (_state, service) = console(&configuration(&provider, &base, "", ""))?;
    let cookie = sign_in(&service, &provider, TokenAnswer::bearer(OPERATOR_TOKEN)).await?;
    Ok((service, cookie))
}

/// A plain form post of `form` to `route` from the console's own page,
/// carrying `cookie`.
pub(crate) fn post(route: &str, form: &str, cookie: &str) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::post(route)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "text/html")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", cookie)
        .body(Body::from(form.to_owned()))?)
}
