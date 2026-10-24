// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The lane itself: absent without a mapping set or when switched off, each
//! answered with its own reason, and its compiler warnings logged once as it
//! loads.

use axum::Router;
use axum::body::Body;
use ferrobridge_server::config::{Config, MappingSettings};
use ferrobridge_server::mappings;
use ferrobridge_server::state::AppState;
use http::{Request, StatusCode, header};
use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use crate::support;

use super::FHIR_JSON;
use super::bundle;
use super::call;
use super::server_settings;

/// Returns the application with no mapping set configured.
fn bare() -> Router {
    ferrobridge_server::router(support::state(), &server_settings())
}

#[tokio::test]
async fn the_operations_answer_unavailable_with_no_mapping_set() -> Result<(), Box<dyn Error>> {
    let (status, media, body) = call(
        bare(),
        Request::post("/fhir/$toopenehr")
            .header(header::CONTENT_TYPE, FHIR_JSON)
            .body(Body::from(bundle()?))?,
    )
    .await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status);
    assert_eq!(FHIR_JSON, media);
    let answer: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(Some("OperationOutcome"), answer["resourceType"].as_str());
    assert_eq!(
        Some("the FHIRconnect operations are not served: no mapping set is configured"),
        answer["issue"][0]["details"]["text"].as_str(),
        "{body}"
    );
    Ok(())
}

/// Returns the reason the application `text` configures answers a
/// `$toopenehr` with, which must be a `503`.
async fn reason_of(text: &str) -> Result<String, Box<dyn Error>> {
    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
    let state = AppState::build(&settings).await?;
    let app = ferrobridge_server::router(Arc::new(state), &server_settings());
    let (status, _media, body) = call(
        app,
        Request::post("/fhir/$toopenehr")
            .header(header::CONTENT_TYPE, FHIR_JSON)
            .body(Body::from(bundle()?))?,
    )
    .await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{body}");
    let answer: serde_json::Value = serde_json::from_str(&body)?;
    Ok(answer["issue"][0]["details"]["text"]
        .as_str()
        .unwrap_or_default()
        .to_owned())
}

#[tokio::test]
async fn a_configuration_with_no_mapping_set_names_that_reason() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        "the FHIRconnect operations are not served: no mapping set is configured",
        reason_of("").await?
    );
    Ok(())
}

#[tokio::test]
async fn a_lane_switched_off_names_the_switch_and_not_a_missing_mapping_set()
-> Result<(), Box<dyn Error>> {
    let root = super::mapping_tree()?;
    let text = format!(
        "[mappings]\ndirectory = {}\ntemplates = {}\n\n[operations]\nenabled = false\n",
        toml_path(&root.path().join("mappings")),
        toml_path(&root.path().join("templates")),
    );
    assert_eq!(
        "the FHIRconnect operations are not served: [operations] enabled is false",
        reason_of(&text).await?
    );
    Ok(())
}

/// Returns `path` as a TOML basic string.
fn toml_path(path: &std::path::Path) -> String {
    format!("{:?}", path.display().to_string())
}

#[test]
fn a_compiler_warning_reaches_the_log_once_as_the_lane_loads() -> Result<(), Box<dyn Error>> {
    // The KDS project context lists an extension of a model no mapping of it
    // reaches, which the compiler accepts with `fc-unreached-extension`.
    let root = tempfile::tempdir()?;
    crate::kds::write_mappings(&root.path().join("mappings"))?;
    std::fs::create_dir_all(root.path().join("templates"))?;
    std::fs::write(
        root.path().join("templates").join("KDS_Diagnose.opt"),
        ferrobridge_testkit::fixtures::KDS_DIAGNOSE_OPT,
    )?;
    let settings = MappingSettings {
        directory: root.path().join("mappings"),
        templates: root.path().join("templates"),
    };
    let logs = support::Logs::default();
    let capture = ferrobridge_server::telemetry::subscriber(
        ferrobridge_server::telemetry::Rendering::Json,
        "warn",
        false,
        logs.clone(),
    );
    let loaded = tracing::subscriber::with_default(capture, || mappings::load(&settings));
    loaded?;
    let text = logs.text();
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("fc-unreached-extension"))
        .collect();
    assert_eq!(lines.len(), 1, "one line per warning: {text}");
    let line: serde_json::Value = serde_json::from_str(lines.first().ok_or("one line")?)?;
    assert_eq!(Some("WARN"), line["level"].as_str(), "{line}");
    assert_eq!(
        Some("ferrobridge_kds_diagnose.context"),
        line["context"].as_str(),
        "{line}"
    );
    assert!(
        line["file"]
            .as_str()
            .is_some_and(|file| file.ends_with("ferrobridge_kds_diagnose.context.yml")),
        "{line}"
    );
    assert!(
        !text.contains("COMPOSITION.report.v1.Condition"),
        "the record names the file and the code, never what the mapping says: {text}"
    );
    Ok(())
}
