// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping set as a ZIP: the import page and its multipart post, which
//! reaches the bridge as the raw archive, and the export download, over a
//! stub bridge.

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode};
use serde_json::json;
use wiremock::matchers::{body_bytes, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::stub::{bridge, signed_in};
use crate::support::{OPERATOR_TOKEN, get_with, send};

/// The bytes the cases post as the archive: no ZIP, which the console never
/// reads and sends as it came.
const ARCHIVE: &[u8] = b"PK\x03\x04synthetic archive bytes";

/// The multipart boundary of the cases' posts.
const BOUNDARY: &str = "ferrobridge-synthetic-boundary";

/// A multipart body with the archive, `reason` and `decision`.
fn multipart(archive: &[u8], reason: &str, decision: &str) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"archive\"; filename=\"set.zip\"\r\nContent-Type: application/zip\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(archive);
    for (name, value) in [("reason", reason), ("decision", decision)] {
        body.extend_from_slice(
            format!(
                "\r\n--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    body
}

/// A multipart post of `body` to the import page, carrying `cookie`.
fn posted(body: Vec<u8>, cookie: &str) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::post("/mappings/import")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .header("accept", "text/html")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", cookie)
        .body(Body::from(body))?)
}

#[tokio::test]
async fn the_import_page_holds_a_multipart_form_with_the_archive_field()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, get_with("/mappings/import", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    for words in [
        r#"enctype="multipart/form-data""#,
        r#"name="archive" type="file""#,
        "Check and save as a revision",
    ] {
        assert!(page.contains(words), "{words}: {page}");
    }
    Ok(())
}

#[tokio::test]
async fn an_import_sends_the_archive_raw_and_links_the_new_revision() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/operator/revisions/import"))
        .and(header("content-type", "application/zip"))
        .and(header("ferrobridge-reason", "synthetic import"))
        .and(header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .and(body_bytes(ARCHIVE.to_vec()))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("etag", "\"4\"")
                .set_body_json(json!({"clean": true, "counts": {"fhirconnect_files": 1, "contexts": 1, "programs": 1, "templates": 1, "omocl_files": 0}, "diagnostics": []})),
        )
        .expect(1)
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(
        &service,
        posted(multipart(ARCHIVE, "synthetic import", "import"), &cookie)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(page.contains("saved as revision 4"), "{page}");
    assert!(page.contains("/mappings/revisions/4"), "{page}");
    Ok(())
}

#[tokio::test]
async fn an_import_with_no_archive_asks_for_one_before_the_bridge_is_asked()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/operator/revisions/import"))
        .respond_with(ResponseTemplate::new(201))
        .expect(0)
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(
        &service,
        posted(multipart(b"", "synthetic", "import"), &cookie)?,
    )
    .await?;
    assert!(page.contains("Choose the ZIP to import."), "{page}");
    Ok(())
}

#[tokio::test]
async fn an_import_the_bridge_refuses_shows_its_diagnostics() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("POST"))
        .and(path("/operator/revisions/import"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "clean": false,
            "counts": {"fhirconnect_files": 1, "contexts": 0, "programs": 0, "templates": 1, "omocl_files": 0},
            "diagnostics": [{"file": "fhirconnect/a.yml", "line": 3, "column": 1, "severity": "error", "code": "invalid-mapping-name", "message": "the synthetic name is refused", "mapping": null, "model_path": ["metadata", "name"]}]
        })))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(
        &service,
        posted(multipart(ARCHIVE, "synthetic import", "import"), &cookie)?,
    )
    .await?;
    assert!(page.contains("Not saved"), "{page}");
    assert!(page.contains("fhirconnect/a.yml"), "{page}");
    Ok(())
}

#[tokio::test]
async fn the_export_downloads_the_bridges_archive() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    Mock::given(method("GET"))
        .and(path("/operator/revisions/3/export"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(ARCHIVE, "application/zip"))
        .mount(&bridge)
        .await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, body) =
        send(&service, get_with("/mappings/revisions/3/export", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        crate::support::header(&response, "content-disposition")
            .contains("ferrobridge-mappings-3.zip")
    );
    assert_eq!(String::from_utf8_lossy(ARCHIVE), body);
    Ok(())
}
