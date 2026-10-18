// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping files against a stub bridge: the list over `GET
//! /operator/mappings`, and one file over `GET /operator/mappings/{path}`,
//! shown verbatim with its line numbers, highlighting and diagnostics.

use std::error::Error;

use axum::body::Body;
use ferrobridge_testkit::oidc::{Provider, TokenAnswer};
use http::{Request, StatusCode};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{OPERATOR_TOKEN, configuration, console, get, get_with, send, sign_in};

/// A synthetic model file with a multi-byte character, characters HTML
/// escapes, a quoted scalar, a comment, a blank line and a final newline.
const MODEL: &str = "grammar: FHIRConnect/v1.0.0\ntype: model\nmetadata:\n  name: synthetic-blood-pressure # naïve <name>\n\n  version: \"0.0.1 & 'b'\"\nspec:\n  - path: \"a<b>\"\n";

/// The file's wire path.
const PATH: &str = "fhirconnect/models/synthetic-bp.model.yml";

/// The listing the stub answers: the file, with one error on line 4.
fn listing() -> serde_json::Value {
    json!([
        {
            "path": PATH,
            "language": "fhirconnect",
            "kind": "model",
            "sha256": "abc123",
            "owner": "provisioned",
            "read_only": true,
            "diagnostics": [{
                "file": PATH, "line": 4, "column": 9,
                "severity": "error", "code": "invalid-mapping-name",
                "message": "the synthetic name is refused",
                "mapping": null, "model_path": ["metadata", "name"]
            }]
        },
        {
            "path": "omocl/measurement.yml",
            "language": "omocl",
            "kind": null,
            "sha256": "def456",
            "owner": "console",
            "read_only": false,
            "diagnostics": []
        }
    ])
}

/// A stub bridge that answers the listing and the file's bytes `bytes`.
async fn bridge(bytes: Vec<u8>) -> MockServer {
    let server = MockServer::start().await;
    let token = format!("Bearer {OPERATOR_TOKEN}");
    Mock::given(method("GET"))
        .and(path("/operator/mappings"))
        .and(header("authorization", token.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(listing()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/operator/mappings/{PATH}")))
        .and(header("authorization", token.as_str()))
        .and(header("accept", "application/yaml"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"abc123\"")
                .set_body_raw(bytes, "application/yaml"),
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

/// The HTML the server function at `route` draws for `form`.
async fn drawn(
    service: &axum::Router,
    route: &str,
    form: &str,
    cookie: &str,
) -> Result<String, Box<dyn Error>> {
    let request = Request::post(route)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", cookie)
        .body(Body::from(form.to_owned()))?;
    let (response, body) = send(service, request).await?;
    assert_eq!(StatusCode::OK, response.status(), "{route}: {body}");
    Ok(serde_json::from_str::<String>(&body)?)
}

/// The text of every line item of the shown file, tags removed and
/// entities read back.
fn shown_lines(html: &str) -> Vec<String> {
    let list = html
        .split_once("fui-code-panel-numbered")
        .and_then(|(_, rest)| rest.split_once("<pre>"))
        .and_then(|(_, rest)| rest.split_once("</pre>"))
        .map_or_else(|| panic!("the file's lines: {html}"), |(list, _)| list);
    list.split("<span class=\"fui-code-line\" id=\"L")
        .skip(1)
        .map(|item| {
            let content = item
                .split_once("\">")
                .map(|(_, content)| content)
                .expect("a line's content");
            let content = content.strip_suffix("</span>").expect("a closed line");
            let mut text = String::new();
            let mut inside = false;
            for character in content.chars() {
                match character {
                    '<' => inside = true,
                    '>' => inside = false,
                    _ if !inside => text.push(character),
                    _ => {}
                }
            }
            text.replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&#39;", "'")
                .replace("&amp;", "&")
        })
        .collect()
}

#[tokio::test]
async fn the_bytes_shown_equal_the_bytes_the_operator_api_serves() -> Result<(), Box<dyn Error>> {
    let bridge = bridge(MODEL.as_bytes().to_vec()).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let html = drawn(
        &service,
        "/api/file",
        &format!(
            "path={}",
            url::form_urlencoded::byte_serialize(PATH.as_bytes()).collect::<String>()
        ),
        &cookie,
    )
    .await?;
    let mut shown = shown_lines(&html).join("\n");
    shown.push('\n');
    assert_eq!(MODEL, shown);
    assert!(
        html.contains(r#"<span class="fui-code-line" id="L4">"#),
        "{html}"
    );
    assert!(html.contains("<code>abc123</code>"), "the digest: {html}");
    assert!(
        html.contains(r#"<span class="fui-tok-key">grammar</span>"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<span class="fui-tok-comment"># naïve &lt;name&gt;</span>"#),
        "{html}"
    );
    assert!(!html.contains("<b>"), "escaped: {html}");
    Ok(())
}

#[tokio::test]
async fn each_diagnostic_sits_beside_the_file_linked_to_its_line() -> Result<(), Box<dyn Error>> {
    let bridge = bridge(MODEL.as_bytes().to_vec()).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let form = format!(
        "path={}",
        url::form_urlencoded::byte_serialize(PATH.as_bytes()).collect::<String>()
    );
    let html = drawn(&service, "/api/file", &form, &cookie).await?;
    assert!(
        html.contains(r##"<a href="#L4">line 4, column 9</a>"##),
        "{html}"
    );
    assert!(html.contains("invalid-mapping-name"), "{html}");
    assert!(html.contains("the synthetic name is refused"), "{html}");
    assert!(
        html.contains(r#"<span class="fui-badge fui-tone-bad">error</span>"#),
        "{html}"
    );
    Ok(())
}

#[tokio::test]
async fn a_file_that_is_not_utf8_is_refused_never_shown_altered() -> Result<(), Box<dyn Error>> {
    let bridge = bridge(vec![b'a', 0xff, b'\n']).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let form = format!(
        "path={}",
        url::form_urlencoded::byte_serialize(PATH.as_bytes()).collect::<String>()
    );
    let html = drawn(&service, "/api/file", &form, &cookie).await?;
    assert!(html.contains("The file is not UTF-8"), "{html}");
    assert!(!html.contains("code-lines"), "{html}");
    Ok(())
}

#[tokio::test]
async fn a_path_that_climbs_is_refused_before_the_bridge_is_asked() -> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for path in ["fhirconnect/../secret", "fhirconnect//a.yml", "", "./a.yml"] {
        let form = format!(
            "path={}",
            url::form_urlencoded::byte_serialize(path.as_bytes()).collect::<String>()
        );
        let html = drawn(&service, "/api/file", &form, &cookie).await?;
        assert!(
            html.contains("not plain names below a root"),
            "{path}: {html}"
        );
    }
    let asked = bridge.received_requests().await.ok_or("recorded")?;
    assert!(
        asked
            .iter()
            .all(|request| request.url.path() == "/operator/mappings"),
        "only the listing was asked for: {:?}",
        asked
            .iter()
            .map(|request| request.url.path().to_owned())
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[tokio::test]
async fn a_file_the_bridge_does_not_serve_draws_its_404() -> Result<(), Box<dyn Error>> {
    let bridge = bridge(MODEL.as_bytes().to_vec()).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let html = drawn(
        &service,
        "/api/file",
        "path=fhirconnect%2Fmodels%2Fabsent.yml",
        &cookie,
    )
    .await?;
    assert!(
        html.contains("404: the bridge refused the request."),
        "{html}"
    );
    Ok(())
}

#[tokio::test]
async fn the_list_draws_every_file_with_its_owner_and_a_link_to_it() -> Result<(), Box<dyn Error>> {
    let bridge = bridge(MODEL.as_bytes().to_vec()).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let html = drawn(&service, "/api/files", "", &cookie).await?;
    assert!(
        html.contains(&format!(r#"<a href="/files/{PATH}">"#)),
        "{html}"
    );
    assert!(
        html.contains("read-only") && html.contains("editable"),
        "{html}"
    );
    assert!(html.contains("1 errors, 0 warnings"), "{html}");
    assert!(
        html.contains("not read"),
        "a file the load did not read: {html}"
    );
    Ok(())
}

#[tokio::test]
async fn the_file_page_carries_the_file_and_a_visitor_reads_the_sign_in()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge(MODEL.as_bytes().to_vec()).await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, get_with(&format!("/files/{PATH}"), &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        page.contains(r#"<span class="fui-tok-key">grammar</span>"#),
        "{page}"
    );
    let (_response, page) = send(&service, get_with("/files", &cookie)?).await?;
    assert!(page.contains(">Mapping files</h1>"), "{page}");
    let (_state, visitor) = console("")?;
    let (_response, page) = send(&visitor, get(&format!("/files/{PATH}"))?).await?;
    assert!(page.contains(">Sign in</a>"), "{page}");
    Ok(())
}
