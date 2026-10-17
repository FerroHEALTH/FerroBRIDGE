// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The installation report to `FerroPULSE`: the reports the server builds,
//! each validated against the vendored envelope v2 schema, the window the
//! request log fills, the sends to a mock collector, and the boot line.
//!
//! The contract is `FerroPULSE` `docs/report-v2.md` at the commit
//! `docs/VERSIONS.md` pins; no other specification governs the report.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::routing::{get, post};
use ferrobridge_hl7v2::ack::Code;
use ferrobridge_server::health::{Check, HealthIndicator, IndicatorState, Registry};
use ferrobridge_server::startup::{Lane, MappingCounts};
use ferrobridge_server::state::AppState;
use ferrobridge_server::telemetry::{Rendering, subscriber};
use ferrobridge_server::usage_report::config::{Deployment, UsageReportSettings};
use ferrobridge_server::usage_report::reporter::{self, Prepared, Reporter, StartOutcome};
use ferrobridge_server::with_middleware;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt as _;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support;

/// The vendored schema tree, `scripts/vendor/ferropulse-schemas.sh`.
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ferropulse/schemas")
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).expect("the vendored file reads");
    serde_json::from_str(&text).expect("the vendored file is JSON")
}

/// The report envelope v2 validator.
fn validator() -> jsonschema::Validator {
    let schema = read_json(&fixtures().join("report-envelope.v2.json"));
    jsonschema::validator_for(&schema).expect("the schema compiles")
}

/// The examples of one validity, by file name.
fn examples(validity: &str) -> Vec<(String, Value)> {
    let dir = fixtures()
        .join("examples/report-envelope.v2")
        .join(validity);
    let mut out: Vec<(String, Value)> = std::fs::read_dir(&dir)
        .expect("the examples are vendored")
        .map(|entry| {
            let path = entry.expect("an entry").path();
            let name = path
                .file_name()
                .expect("a name")
                .to_string_lossy()
                .into_owned();
            (name, read_json(&path))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Fails unless `report` is valid against the vendored schema.
fn assert_valid(report: &Value) {
    let errors: Vec<String> = validator()
        .iter_errors(report)
        .map(|error| error.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}\n{report:#}");
}

/// `report` with the members that depend on the host or the build replaced
/// by `*`, after checking it against the schema.
fn stable(mut report: Value) -> Value {
    assert_valid(&report);
    let object = report.as_object_mut().expect("an object");
    let id = object["instance_id"].as_str().expect("a string");
    assert_eq!(id.len(), 36, "hyphenated: {id}");
    for member in [
        "instance_id",
        "git_sha",
        "uptime_s",
        "cpu_bucket",
        "memory_bucket",
    ] {
        object.insert(member.to_owned(), json!("*"));
    }
    report
}

/// An indicator that answers the same state every time.
#[derive(Debug)]
struct Fixed {
    name: &'static str,
    up: bool,
}

impl HealthIndicator for Fixed {
    fn name(&self) -> &'static str {
        self.name
    }

    fn check(&self) -> Check<'_> {
        let up = self.up;
        Box::pin(async move {
            if up {
                IndicatorState::up()
            } else {
                IndicatorState::down("the synthetic upstream is down")
            }
        })
    }
}

/// A lane as the boot summary reports it.
fn lane(name: &'static str, enabled: bool, programs: Option<usize>) -> Lane {
    Lane {
        name,
        enabled,
        mappings: programs.map(|programs| MappingCounts {
            contexts: programs,
            programs,
            templates: 1,
        }),
        ..Lane::default()
    }
}

/// A state with a CDR that answers, a terminology server that does not, the
/// facade and the operations on, and the HL7 v2 face and the ETL off.
fn state() -> Arc<AppState> {
    let health = Registry::new(vec![
        Arc::new(Fixed {
            name: "cdr",
            up: true,
        }),
        Arc::new(Fixed {
            name: "terminology",
            up: false,
        }),
    ]);
    Arc::new(AppState::with_health(health).with_lanes(vec![
        lane("facade", true, Some(12)),
        lane("hl7v2", false, None),
        lane("operations", true, Some(3)),
        lane("etl", false, None),
        lane("terminology", true, None),
    ]))
}

/// A report configuration over `state_dir` posting to `endpoint`.
fn config(state_dir: &Path, endpoint: &str) -> UsageReportSettings {
    UsageReportSettings {
        enabled: true,
        endpoint: url::Url::parse(endpoint).expect("a URL"),
        state_dir: Some(state_dir.to_path_buf()),
        deployment: Deployment::Compose,
    }
}

fn prepared(config: &UsageReportSettings) -> Prepared {
    reporter::prepare(config, None).expect("the report runs")
}

fn reporter_over(config: &UsageReportSettings, state: Arc<AppState>) -> Reporter {
    Reporter::new(config, prepared(config), state, Instant::now()).expect("the client builds")
}

#[test]
fn the_validator_admits_every_valid_example_and_refuses_every_invalid_one() {
    let validator = validator();
    let valid = examples("valid");
    let invalid = examples("invalid");
    assert!(
        !valid.is_empty() && !invalid.is_empty(),
        "the examples are vendored"
    );
    for (name, example) in valid {
        assert!(validator.is_valid(&example), "{name} should be valid");
    }
    for (name, example) in invalid {
        assert!(!validator.is_valid(&example), "{name} should be refused");
    }
}

#[tokio::test]
async fn the_start_report_is_exactly_the_v2_envelope() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = config(dir.path(), "https://report.ferropulse.eu/v1/report");
    let reporter = reporter_over(&config, state());
    let report = serde_json::to_value(reporter.start_report().await).expect("serializes");
    assert_eq!(
        stable(report),
        json!({
            "schema": 2,
            "event": "start",
            "first_start": true,
            "product": "ferrobridge",
            "instance_id": "*",
            "version": env!("CARGO_PKG_VERSION"),
            "git_sha": "*",
            "licence": "non-commercial",
            "deployment": "compose",
            "uptime_s": "*",
            "cpu_bucket": "*",
            "memory_bucket": "*",
            "status": {
                "lanes": ["facade", "operations", "terminology"],
                "auth": false,
                "facade_mappings": "11-50",
                "operations_mappings": "1-10",
                "cdr_ok": true,
                "terminology_ok": false
            }
        })
    );
}

#[tokio::test]
async fn a_server_with_no_lane_reports_an_empty_status_and_an_empty_window() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = config(dir.path(), "https://report.ferropulse.eu/v1/report");
    let reporter = reporter_over(&config, support::state());
    let counts = support::state().usage_window().take();
    let report = serde_json::to_value(reporter.daily_report(&counts).await).expect("serializes");
    let report = stable(report);
    assert_eq!(report["status"], json!({ "lanes": [], "auth": false }));
    assert_eq!(
        report["metrics"],
        json!({
            "window_24h": {
                "requests_bucket": "0",
                "groups": {},
                "counters": {
                    "responses_4xx": 0,
                    "hl7v2_aa": 0,
                    "hl7v2_ae": 0,
                    "hl7v2_ar": 0,
                    "mapping_failures": 0
                }
            }
        })
    );
    assert!(report.get("first_start").is_none(), "start only");
}

#[tokio::test]
async fn the_daily_report_counts_hl7v2_messages_and_mapping_failures() {
    let state = state();
    let window = state.usage_window();
    window.record_message(Duration::from_millis(40), Some(Code::Accept));
    window.record_message(Duration::from_millis(60), Some(Code::Accept));
    window.record_message(Duration::from_millis(3), Some(Code::Error));
    window.record_message(Duration::from_millis(1), None);
    window.record_acknowledgment(Code::Reject);
    window.record_mapping_failure();
    window.record_mapping_failure();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = config(dir.path(), "https://report.ferropulse.eu/v1/report");
    let reporter = reporter_over(&config, Arc::clone(&state));
    let counts = state.usage_window().take();
    let report =
        stable(serde_json::to_value(reporter.daily_report(&counts).await).expect("serializes"));
    let window = &report["metrics"]["window_24h"];
    assert_eq!(window["requests_bucket"], json!("1-100"));
    assert_eq!(
        window["counters"],
        json!({
            "responses_4xx": 0,
            "hl7v2_aa": 2,
            "hl7v2_ae": 1,
            "hl7v2_ar": 1,
            "mapping_failures": 2
        })
    );
    let groups = window["groups"].as_object().expect("an object");
    assert_eq!(
        groups.keys().map(String::as_str).collect::<Vec<_>>(),
        ["hl7v2_message"]
    );
    assert_eq!(
        groups["hl7v2_message"]["histogram"],
        json!([2, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0])
    );
    assert_eq!(groups["hl7v2_message"]["errors_5xx"], json!(0));
}

/// Sends `request` through `app` and returns its status.
async fn send(app: &Router, request: Request<Body>) -> StatusCode {
    app.clone()
        .oneshot(request)
        .await
        .expect("an answer")
        .status()
}

#[tokio::test]
async fn the_daily_report_counts_requests_by_route_group_and_never_by_path() {
    let state = state();
    let routes = Router::new()
        .route(
            "/fhir/{resource_type}/{id}",
            get(|| async { StatusCode::OK }),
        )
        .route(
            "/fhir/$tofhir",
            post(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
        )
        .route("/health/liveness", get(|| async { StatusCode::OK }));
    let app = with_middleware(routes, Arc::clone(&state), &support::settings());
    let get_request = |uri: &str| Request::get(uri).body(Body::empty()).expect("a request");
    for _ in 0..2 {
        let status = send(&app, get_request("/fhir/Condition/SYNTHETIC-ID-1")).await;
        assert_eq!(StatusCode::OK, status);
    }
    let tofhir = Request::post("/fhir/$tofhir")
        .body(Body::empty())
        .expect("a request");
    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, send(&app, tofhir).await);
    assert_eq!(
        StatusCode::OK,
        send(&app, get_request("/health/liveness")).await
    );
    assert_eq!(
        StatusCode::NOT_FOUND,
        send(&app, get_request("/SYNTHETIC-UNMATCHED")).await
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let config = config(dir.path(), "https://report.ferropulse.eu/v1/report");
    let reporter = reporter_over(&config, Arc::clone(&state));
    let counts = state.usage_window().take();
    let report = serde_json::to_value(reporter.daily_report(&counts).await).expect("serializes");
    let rendered = report.to_string();
    assert!(
        !rendered.contains("SYNTHETIC"),
        "no path reaches a report: {rendered}"
    );
    assert!(
        !rendered.contains("Condition"),
        "no resource type either: {rendered}"
    );
    let report = stable(report);
    let window = &report["metrics"]["window_24h"];
    assert_eq!(window["requests_bucket"], json!("1-100"));
    assert_eq!(
        window["counters"],
        json!({
            "responses_4xx": 1,
            "hl7v2_aa": 0,
            "hl7v2_ae": 0,
            "hl7v2_ar": 0,
            "mapping_failures": 0
        })
    );
    let groups = window["groups"].as_object().expect("an object");
    assert_eq!(
        groups.keys().map(String::as_str).collect::<Vec<_>>(),
        ["facade_read", "operations_to_fhir", "other"],
        "the probes are not counted and a group without requests is left out"
    );
    let total = |group: &str| -> u64 {
        groups[group]["histogram"]
            .as_array()
            .expect("an array")
            .iter()
            .map(|count| count.as_u64().expect("a count"))
            .sum()
    };
    assert_eq!(total("facade_read"), 2);
    assert_eq!(total("operations_to_fhir"), 1);
    assert_eq!(total("other"), 1);
    assert_eq!(groups["facade_read"]["errors_5xx"], json!(0));
    assert_eq!(groups["operations_to_fhir"]["errors_5xx"], json!(1));
}

#[tokio::test]
async fn the_start_report_reaches_the_collector_once_per_ten_minutes() {
    let collector = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/report"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&collector)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = config(dir.path(), &format!("{}/v1/report", collector.uri()));
    let reporter = reporter_over(&config, state());
    let first = reporter.send_start().await.expect("sent");
    assert_eq!(first, StartOutcome::Sent(StatusCode::NO_CONTENT));
    let again = reporter.send_start().await.expect("decided");
    assert_eq!(again, StartOutcome::RateLimited);
    let received = collector.received_requests().await.expect("recorded");
    assert_eq!(1, received.len(), "one start report");
    let request = received.first().expect("one start report");
    let user_agent = request
        .headers
        .get("user-agent")
        .and_then(|value| value.to_str().ok())
        .expect("a user agent");
    assert_eq!(
        user_agent,
        format!("ferrobridge/{}", env!("CARGO_PKG_VERSION"))
    );
    let body: Value = serde_json::from_slice(&request.body).expect("a JSON body");
    assert_eq!(body["event"], json!("start"));
    assert_valid(&body);
}

#[tokio::test]
async fn a_daily_report_the_collector_answers_ends_its_window_even_when_refused() {
    let collector = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/report"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&collector)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = config(dir.path(), &format!("{}/v1/report", collector.uri()));
    let state = state();
    state.usage_window().record(
        &http::Method::GET,
        Some("/fhir/metadata"),
        StatusCode::OK,
        Duration::from_millis(2),
    );
    let reporter = reporter_over(&config, Arc::clone(&state));
    let status = reporter.send_daily().await.expect("answered");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(state.usage_window().take().requests(), 0);
    let received = collector.received_requests().await.expect("recorded");
    let body: Value =
        serde_json::from_slice(&received.first().expect("one report").body).expect("JSON");
    assert_eq!(body["event"], json!("daily"));
    assert!(body["metrics"]["window_24h"]["groups"]["facade_metadata"].is_object());
    assert_valid(&body);
}

#[tokio::test]
async fn a_daily_report_that_reaches_no_collector_keeps_its_window() {
    let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = closed.local_addr().expect("an address").port();
    drop(closed);
    let dir = tempfile::tempdir().expect("tempdir");
    let config = config(dir.path(), &format!("http://127.0.0.1:{port}/v1/report"));
    let state = state();
    state.usage_window().record(
        &http::Method::GET,
        Some("/fhir/metadata"),
        StatusCode::OK,
        Duration::from_millis(2),
    );
    let reporter = reporter_over(&config, Arc::clone(&state));
    let error = reporter.send_daily().await.expect_err("nothing listens");
    assert!(
        matches!(error, reporter::ReportError::Request(_)),
        "{error:?}"
    );
    assert_eq!(
        state.usage_window().take().requests(),
        1,
        "the retry carries the window"
    );
}

/// Runs `body` on a current-thread runtime with every log line kept in
/// `logs`, so the task the report spawns logs into it too.
fn with_logs<F: Future<Output = ()>>(logs: &support::Logs, body: impl FnOnce() -> F) {
    let capture = subscriber(Rendering::Json, "info", false, logs.clone());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    tracing::subscriber::with_default(capture, || runtime.block_on(body()));
}

#[test]
fn start_logs_the_boot_line_and_sends_the_start_report() {
    let logs = support::Logs::default();
    with_logs(&logs, || async {
        let collector = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/report"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&collector)
            .await;
        let dir = tempfile::tempdir().expect("tempdir");
        let config = config(dir.path(), &format!("{}/v1/report", collector.uri()));
        let handle = reporter::start(&config, state(), Instant::now(), None)
            .await
            .expect("the report runs");
        let deadline = Instant::now() + Duration::from_secs(10);
        while collector
            .received_requests()
            .await
            .is_none_or(|received| received.is_empty())
        {
            assert!(Instant::now() < deadline, "the start report did not arrive");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        handle.abort();
    });
    let text = logs.text();
    assert!(text.contains("usage report ON: "), "{text}");
    assert!(
        text.contains("Off: FERROBRIDGE__USAGE_REPORT__ENABLED=false"),
        "{text}"
    );
}

#[test]
fn a_ci_environment_keeps_the_report_off_and_writes_nothing() {
    let logs = support::Logs::default();
    let dir = tempfile::tempdir().expect("tempdir");
    let state_dir = dir.path().join("state");
    with_logs(&logs, || async {
        let config = config(&state_dir, "https://report.ferropulse.eu/v1/report");
        let handle =
            reporter::start(&config, state(), Instant::now(), Some("GITHUB_ACTIONS")).await;
        assert!(handle.is_none(), "an explicit on does not override CI");
    });
    let text = logs.text();
    assert!(
        text.contains("usage report OFF: CI detected (GITHUB_ACTIONS); nothing is sent"),
        "{text}"
    );
    assert!(
        !state_dir.exists(),
        "nothing is written while the report is off"
    );
}

#[test]
fn a_switched_off_report_says_so_and_sends_nothing() {
    let logs = support::Logs::default();
    let dir = tempfile::tempdir().expect("tempdir");
    with_logs(&logs, || async {
        let config = UsageReportSettings {
            enabled: false,
            ..config(dir.path(), "https://report.ferropulse.eu/v1/report")
        };
        assert!(
            reporter::start(&config, state(), Instant::now(), None)
                .await
                .is_none()
        );
    });
    let text = logs.text();
    assert!(
        text.contains("usage report OFF: switched off (FERROBRIDGE__USAGE_REPORT__ENABLED=false)"),
        "{text}"
    );
}
