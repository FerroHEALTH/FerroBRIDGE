// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Mapping set against a stub bridge: its tabs over the running load and
//! the listing, the Files tab's find field, filters and pager in the URL,
//! the reload as a plain form post whose outcome the page draws, and the
//! operator API's off line.

use std::error::Error;

use http::StatusCode;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::stub::{BROKEN, CONTEXT, MODEL, OMOCL, answer, bridge, post, signed_in};
use crate::support::{get_with, send};

#[tokio::test]
async fn the_contexts_tab_lists_each_context_with_its_owner_state_and_row_menu()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, body) = send(&service, get_with("/mappings", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(body.contains("synthetic-observation"), "{body}");
    assert!(body.contains("Observation"), "{body}");
    assert!(
        body.contains(&format!("/mappings/files/{CONTEXT}?tab=program")),
        "the row opens the program: {body}"
    );
    assert!(body.contains("Actions for synthetic-observation"), "{body}");
    assert!(body.contains("The running set does not compile"), "{body}");
    assert!(body.contains("Reload the set"), "{body}");
    assert!(
        body.contains(r#"action="/mappings" class"#),
        "the reload is a plain form post: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn the_diagnostics_tab_groups_the_load_by_file_escaped() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, body) = send(&service, get_with("/mappings/diagnostics", &cookie)?).await?;
    assert!(body.contains(BROKEN), "{body}");
    assert!(body.contains("missing-key"), "{body}");
    assert!(!body.contains("<script>alert(1)</script>"), "{body}");
    assert!(body.contains("&lt;script&gt;"), "{body}");
    Ok(())
}

#[tokio::test]
async fn the_files_tab_filters_by_the_query_and_says_what_it_shows() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, every) = send(&service, get_with("/mappings/files", &cookie)?).await?;
    for file in [CONTEXT, MODEL, BROKEN, OMOCL] {
        assert!(every.contains(file), "{file}: {every}");
    }
    let (_response, console_files) = send(
        &service,
        get_with("/mappings/files?owner=console", &cookie)?,
    )
    .await?;
    assert!(console_files.contains(CONTEXT), "{console_files}");
    assert!(!console_files.contains(MODEL), "{console_files}");
    assert!(console_files.contains("Owner: console"), "{console_files}");
    assert!(
        console_files.contains("Owner · 1 of 2"),
        "the filter menu says what it keeps: {console_files}"
    );
    let (_response, both) = send(
        &service,
        get_with("/mappings/files?owner=console&owner=provisioned", &cookie)?,
    )
    .await?;
    assert!(
        both.contains(MODEL),
        "every owner kept is no filter: {both}"
    );
    assert!(!both.contains("Owner: "), "{both}");
    let (_response, none) = send(
        &service,
        get_with("/mappings/files?q=nothing-matches", &cookie)?,
    )
    .await?;
    assert!(
        none.contains("No mapping file matches these filters."),
        "{none}"
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_tab_is_a_404() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, _body) = send(&service, get_with("/mappings/nowhere", &cookie)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

#[tokio::test]
async fn a_reload_posts_and_the_page_draws_whether_it_replaced_the_set()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/operator/reload"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "clean": true,
            "counts": {"fhirconnect_files": 3, "contexts": 1, "programs": 1, "templates": 2, "omocl_files": 1},
            "diagnostics": []
        })))
        .expect(1)
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, body) = send(&service, post("/mappings", "", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("Reloaded: the set compiled and replaced the running one"),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_reload_draws_its_diagnostics_and_the_set_stands() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/operator/reload"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "clean": false,
            "counts": {"fhirconnect_files": 3, "contexts": 1, "programs": 0, "templates": 2, "omocl_files": 1},
            "diagnostics": [{
                "file": BROKEN, "line": 2, "column": 1, "severity": "error", "code": "unknown-archetype",
                "message": "the synthetic archetype is not in the template", "mapping": null, "model_path": []
            }]
        })))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, body) = send(&service, post("/mappings", "", &cookie)?).await?;
    assert!(body.contains("Not reloaded"), "{body}");
    assert!(body.contains("unknown-archetype"), "{body}");
    assert!(body.contains("Diagnostics of the reload"), "{body}");
    Ok(())
}

#[tokio::test]
async fn an_operator_api_that_is_off_says_so_in_one_line() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    answer(&server, "/health/info", 200, &crate::stub::info()).await;
    answer(&server, "/operator/state", 404, &json!({})).await;
    answer(&server, "/operator/mappings", 404, &json!({})).await;
    let (service, cookie) = signed_in(&server).await?;
    let (response, body) = send(&service, get_with("/mappings", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        body.contains("The operator API is off on this bridge"),
        "{body}"
    );
    assert!(
        body.contains("Not known: the operator API is off on this bridge"),
        "the navigation's count says why it has none: {body}"
    );
    Ok(())
}
