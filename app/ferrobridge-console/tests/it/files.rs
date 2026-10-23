// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One mapping file and one template against a stub bridge: the file shown
//! byte for byte beside its diagnostics, its Program tab, the lock of a
//! provisioned file, a file that is not UTF-8 refused and never shown
//! altered, a file the bridge does not serve, and a template's contexts.

use std::error::Error;

use http::StatusCode;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::stub::{BROKEN, CONTEXT, MODEL, bridge, signed_in};
use crate::support::{get_with, send};

#[tokio::test]
async fn a_context_file_shows_its_lines_its_digest_and_edit() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(
        &service,
        get_with(&format!("/mappings/files/{CONTEXT}"), &cookie)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(page.contains(r#"id="L1""#), "numbered lines: {page}");
    assert!(page.contains("synthetic-observation"), "{page}");
    assert!(page.contains("SHA-256 5712e3f7…f2e0e3fa"), "{page}");
    assert!(
        page.contains(&format!("/mappings/edit/{CONTEXT}")),
        "{page}"
    );
    assert!(
        page.contains("No diagnostics: the running load raised none"),
        "{page}"
    );
    assert!(page.contains("Program"), "the context's tabs: {page}");
    Ok(())
}

#[tokio::test]
async fn the_program_tab_says_what_the_context_compiled() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(
        &service,
        get_with(&format!("/mappings/files/{CONTEXT}?tab=program"), &cookie)?,
    )
    .await?;
    assert!(page.contains("The compiled program"), "{page}");
    assert!(page.contains("synthetic-blood-pressure"), "{page}");
    assert!(page.contains("synthetic-observation|1.0.0"), "{page}");
    assert!(page.contains("No search file names this context"), "{page}");
    Ok(())
}

#[tokio::test]
async fn a_provisioned_file_is_locked_and_offers_check_a_draft() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("GET"))
        .and(path(format!("/operator/mappings/{MODEL}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw("type: model\n", "application/yaml"))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(
        &service,
        get_with(&format!("/mappings/files/{MODEL}"), &cookie)?,
    )
    .await?;
    assert!(page.contains("Provisioned, so read-only here"), "{page}");
    assert!(page.contains("Check a draft"), "{page}");
    assert!(
        page.contains("Used by"),
        "a model lists its contexts: {page}"
    );
    Ok(())
}

#[tokio::test]
async fn a_file_with_a_diagnostic_draws_it_under_its_line_and_in_the_list()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("GET"))
        .and(path(format!("/operator/mappings/{BROKEN}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("a: 1\nb: 2\nc: 3\nd: 4\n", "application/yaml"),
        )
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(
        &service,
        get_with(&format!("/mappings/files/{BROKEN}"), &cookie)?,
    )
    .await?;
    assert!(page.contains("missing-key"), "{page}");
    assert!(page.contains("fui-code-diag"), "under its line: {page}");
    assert!(page.contains("#L4"), "linked to its line: {page}");
    assert!(!page.contains("<script>alert(1)</script>"), "{page}");
    Ok(())
}

#[tokio::test]
async fn a_file_that_is_not_utf8_is_refused_never_shown_altered() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("GET"))
        .and(path(format!("/operator/mappings/{BROKEN}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0x61, 0xff, 0xfe, 0x62]))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(
        &service,
        get_with(&format!("/mappings/files/{BROKEN}"), &cookie)?,
    )
    .await?;
    assert!(
        page.contains("The console does not show this file"),
        "{page}"
    );
    assert!(page.contains("Not UTF-8"), "{page}");
    assert!(!page.contains('\u{fffd}'), "never shown altered");
    Ok(())
}

#[tokio::test]
async fn a_file_the_bridge_does_not_serve_is_a_404_page() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("GET"))
        .and(path("/operator/mappings/fhirconnect/missing.yml"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"resourceType": "OperationOutcome", "issue": []})),
        )
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(
        &service,
        get_with("/mappings/files/fhirconnect/missing.yml", &cookie)?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status(), "{page}");
    assert!(page.contains("Page not found"), "{page}");
    Ok(())
}

#[tokio::test]
async fn a_path_that_climbs_is_refused_before_the_bridge_is_asked() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for climbing in [
        "/mappings/files/fhirconnect/%2E%2E/secret",
        "/mappings/files/fhirconnect/../secret",
        "/mappings/files/fhirconnect/./secret",
        "/mappings/files/fhirconnect//secret",
    ] {
        let (_response, page) = send(&service, get_with(climbing, &cookie)?).await?;
        assert!(
            page.contains("The mapping file path is not plain names.")
                || page.contains("Page not found"),
            "{climbing}: {page}"
        );
    }
    let asked = bridge.received_requests().await.ok_or("recorded")?;
    let climbed = asked
        .iter()
        .map(|request| request.url.path().to_owned())
        .filter(|path| {
            path.split('/').skip(1).any(|segment| {
                matches!(segment, "" | "." | "..") || segment.to_lowercase().contains("%2e")
            })
        })
        .collect::<Vec<_>>();
    assert!(climbed.is_empty(), "the bridge was asked {climbed:?}");
    Ok(())
}

#[tokio::test]
async fn a_template_lists_the_contexts_that_name_it() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(
        &service,
        get_with("/mappings/templates/synthetic.vital_signs.v1", &cookie)?,
    )
    .await?;
    assert!(page.contains("synthetic-observation"), "{page}");
    assert!(page.contains("templates/vital_signs.opt"), "{page}");
    let (response, _page) = send(
        &service,
        get_with("/mappings/templates/synthetic.missing.v1", &cookie)?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}
