// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dashboard, the faces and the connections against a stub bridge: the
//! stats over the running load and the readiness, Needs attention while an
//! indicator is down, the flow, the FHIR facade's capability and SMART
//! discovery, the HL7 v2 listener, the OMOP ETL, and each upstream's state.

use std::error::Error;

use http::StatusCode;
use serde_json::json;
use wiremock::MockServer;

use crate::stub::{CONTEXT, answer, bridge, info, readiness, signed_in};
use crate::support::{get_with, send};

#[tokio::test]
async fn the_dashboard_draws_its_stats_needs_attention_and_the_flow() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/", &cookie)?).await?;
    assert!(page.contains("Contexts loaded"), "{page}");
    assert!(page.contains("1 of 1"), "{page}");
    assert!(page.contains("1 error"), "{page}");
    assert!(
        page.contains("2 of 3"),
        "two of three upstreams answer: {page}"
    );
    assert!(page.contains("Needs attention"), "{page}");
    assert!(page.contains("answers 503"), "{page}");
    assert!(page.contains("fui-pipeline"), "the flow: {page}");
    assert!(page.contains("/connections/terminology"), "{page}");
    Ok(())
}

#[tokio::test]
async fn a_dashboard_with_every_indicator_up_needs_no_attention() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    answer(&server, "/health/info", 200, &info()).await;
    answer(
        &server,
        "/health/readiness",
        200,
        &json!({"state": "up", "indicators": {"cdr": {"state": "up"}, "terminology": {"state": "up"}, "cdm": {"state": "up"}}}),
    )
    .await;
    answer(&server, "/operator/state", 200, &crate::stub::state()).await;
    let (service, cookie) = signed_in(&server).await?;
    let (_response, page) = send(&service, get_with("/", &cookie)?).await?;
    assert!(!page.contains("Needs attention"), "{page}");
    assert!(page.contains("All up"), "{page}");
    Ok(())
}

#[tokio::test]
async fn the_facade_lists_its_resource_types_each_opening_its_program() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/facade/capability", &cookie)?).await?;
    assert!(page.contains("Observation"), "{page}");
    assert!(page.contains("search-type"), "{page}");
    assert!(page.contains("$validate"), "{page}");
    assert!(
        page.contains(&format!("/mappings/files/{CONTEXT}?tab=program")),
        "{page}"
    );
    let (_response, security) = send(&service, get_with("/facade/security", &cookie)?).await?;
    assert!(
        security.contains("https://idp.example.org/realms/bridge"),
        "{security}"
    );
    let (response, _body) = send(&service, get_with("/facade/nowhere", &cookie)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

#[tokio::test]
async fn a_facade_that_is_off_keeps_overview_alone() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    let mut off = info();
    off["lanes"] = json!([{"name": "facade", "enabled": false}]);
    answer(&server, "/health/info", 200, &off).await;
    answer(&server, "/health/readiness", 200, &readiness()).await;
    let (service, cookie) = signed_in(&server).await?;
    let (_response, page) = send(&service, get_with("/facade", &cookie)?).await?;
    assert!(
        page.contains("Off, so the bridge serves no FHIR route."),
        "{page}"
    );
    assert!(!page.contains("Capability"), "{page}");
    Ok(())
}

#[tokio::test]
async fn the_hl7v2_face_and_the_etl_say_what_they_run() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, hl7) = send(&service, get_with("/hl7v2", &cookie)?).await?;
    assert!(hl7.contains("0.0.0.0:2575"), "{hl7}");
    assert!(hl7.contains("over MLLP"), "{hl7}");
    let (_response, omop) = send(&service, get_with("/omop", &cookie)?).await?;
    assert!(omop.contains("ferrobridge etl run"), "{omop}");
    assert!(omop.contains("Configured"), "{omop}");
    Ok(())
}

#[tokio::test]
async fn each_upstream_carries_its_indicator_and_the_lanes_that_use_it()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, list) = send(&service, get_with("/connections", &cookie)?).await?;
    for upstream in ["openEHR CDR", "Terminology server", "OMOP CDM database"] {
        assert!(list.contains(upstream), "{upstream}: {list}");
    }
    assert!(list.contains("Unreachable"), "{list}");
    let (_response, term) = send(&service, get_with("/connections/terminology", &cookie)?).await?;
    assert!(term.contains("answers 503"), "{term}");
    assert!(term.contains("FHIR facade"), "{term}");
    let (response, _body) = send(&service, get_with("/connections/nowhere", &cookie)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}
