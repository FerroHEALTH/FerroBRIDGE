// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `$tofhir` runs the facade suites read a mapping set back through.

use crate::support;
use axum::body::Body;
use ferrobridge_server::state::AppState;
use ferrobridge_server::state::OperationsLane;
use ferrohealth_health::registry::Registry;
use http::Request;
use http::StatusCode;
use http::header;
use std::error::Error;
use std::sync::Arc;
use tower::ServiceExt as _;

/// Returns the Bundle `$tofhir` answers for `composition`, run by the
/// operations lane over the mapping set under `fixtures` and the operational
/// template `template`, with the suite's subject as `context.patient`.
pub(crate) async fn tofhir(
    fixtures: &str,
    template: &str,
    composition: &serde_json::Value,
) -> Result<serde_json::Value, Box<dyn Error>> {
    tofhir_over(std::path::PathBuf::from(fixtures), template, composition).await
}

/// Returns the Bundle `$tofhir` answers for `composition` over the files
/// `files` of the mapping set under `fixtures` alone, so one context of a set
/// that holds several over one template answers it.
pub(crate) async fn tofhir_with(
    fixtures: &str,
    files: &[&str],
    template: &str,
    composition: &serde_json::Value,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    for file in files {
        std::fs::copy(
            std::path::Path::new(fixtures).join(file),
            root.path().join(file),
        )?;
    }
    tofhir_over(root.path().to_path_buf(), template, composition).await
}

/// Returns the Bundle `$tofhir` answers for `composition` over the mapping
/// set under `directory`.
async fn tofhir_over(
    directory: std::path::PathBuf,
    template: &str,
    composition: &serde_json::Value,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let templates = root.path().join("templates");
    std::fs::create_dir_all(&templates)?;
    std::fs::write(templates.join("template.opt"), template)?;
    let settings = ferrobridge_server::config::MappingSettings {
        directory,
        templates,
    };
    let lane = OperationsLane::new(
        ferrobridge_server::mappings::load(&settings)?,
        "Device/ferrobridge-test",
    );
    let state = Arc::new(AppState::with_health(Registry::default()).serving_operations(lane));
    let app = ferrobridge_server::router(
        state,
        &ferrohealth_server::section::ServerSection {
            body_limit_bytes: 1024 * 1024,
            ..support::settings()
        },
    );
    let request = serde_json::json!({
        "resourceType": "Parameters",
        "parameter": [
            { "name": "composition", "valueString": composition.to_string() },
            {
                "name": "context",
                "part": [{ "name": "patient", "valueReference": { "reference": "Patient/p-1" } }],
            },
        ],
    });
    let response = app
        .oneshot(
            Request::post("/fhir/$tofhir")
                .header(header::CONTENT_TYPE, "application/fhir+json")
                .body(Body::from(request.to_string()))?,
        )
        .await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let answer: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(StatusCode::OK, status, "{answer}");
    Ok(answer)
}

/// Returns the ids of the resources of `resource_type` a Bundle carries, in
/// entry order.
pub(crate) fn ids_of_type(bundle: &serde_json::Value, resource_type: &str) -> Vec<String> {
    bundle["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry["resource"]["resourceType"] == resource_type)
                .filter_map(|entry| entry["resource"]["id"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
