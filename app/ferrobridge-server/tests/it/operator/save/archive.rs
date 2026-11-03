// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The export of a revision as a ZIP and the import of one as an unapplied,
//! checked revision. No specification governs the operator surface: our own
//! design.

use std::error::Error;
use std::io::Write as _;

use axum::body::Body;
use ferrobridge_operator_api::revisions::{Origin, Revisions};
use http::{Request, StatusCode, header};

use super::{BROKEN, CONTEXT, Case, MODEL, REASON};
use crate::operator::bytes;

impl Case {
    /// Saves the model and the context, three revisions in all.
    async fn saved(&self) -> Result<(), Box<dyn Error>> {
        for (name, text) in [
            ("ferrobridge_console.yml", MODEL),
            ("ferrobridge_console.context.yml", CONTEXT),
        ] {
            assert_eq!(
                StatusCode::OK,
                self.create(name, text).await?.status(),
                "{name} saves"
            );
        }
        Ok(())
    }

    /// Returns the export of the revision `id`.
    async fn export(&self, id: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        let response = self
            .send(Request::get(format!("/operator/revisions/{id}/export")).body(Body::empty())?)
            .await?;
        assert_eq!(StatusCode::OK, response.status(), "the export answers");
        assert_eq!(
            Some("application/zip"),
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            "an export is a ZIP"
        );
        bytes(response).await
    }

    /// Posts `archive` as an import with `content_type`.
    async fn import(
        &self,
        archive: Vec<u8>,
        content_type: &str,
    ) -> Result<http::Response<Body>, Box<dyn Error>> {
        self.send(
            Request::post("/operator/revisions/import")
                .header(header::CONTENT_TYPE, content_type)
                .header("ferrobridge-reason", REASON)
                .body(Body::from(archive))?,
        )
        .await
    }

    /// Returns the revision listing.
    async fn listed(&self) -> Result<Revisions, Box<dyn Error>> {
        let response = self
            .send(Request::get("/operator/revisions").body(Body::empty())?)
            .await?;
        Ok(serde_json::from_slice(&bytes(response).await?)?)
    }
}

/// Returns a ZIP of `entries` written by a third party, with today's times.
fn zipped(entries: &[(&str, &str)]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, text) in entries {
        writer.start_file(*name, zip::write::SimpleFileOptions::default())?;
        writer.write_all(text.as_bytes())?;
    }
    Ok(writer.finish()?.into_inner())
}

#[tokio::test]
async fn an_export_and_its_import_round_trip_byte_identically() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.saved().await?;
    let exported = case.export("3").await?;
    assert_eq!(
        exported,
        case.export("3").await?,
        "an export is deterministic"
    );
    let response = case.import(exported.clone(), "application/zip").await?;
    assert_eq!(StatusCode::CREATED, response.status());
    assert_eq!(
        Some("/operator/revisions/4"),
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
    );
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(true, report["clean"], "{report}");
    let listed = case.listed().await?;
    assert_eq!(
        Some(String::from("3")),
        listed.running,
        "an import runs nothing"
    );
    let imported = &listed.revisions[0];
    assert_eq!(
        ("4", Origin::Import, false),
        (imported.id.as_str(), imported.origin, imported.applied)
    );
    assert_eq!(
        exported,
        case.export("4").await?,
        "the round trip is byte-identical"
    );
    Ok(())
}

#[tokio::test]
async fn an_import_that_does_not_compile_is_refused_with_its_diagnostics()
-> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.saved().await?;
    let archive = zipped(&[
        ("fhirconnect/ferrobridge_console.yml", BROKEN),
        ("fhirconnect/ferrobridge_console.context.yml", CONTEXT),
    ])?;
    let response = case.import(archive, "application/zip").await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, response.status());
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert!(
        report["diagnostics"]
            .as_array()
            .ok_or("diagnostics")?
            .iter()
            .any(|diagnostic| diagnostic["file"] == "fhirconnect/ferrobridge_console.yml"),
        "{report}"
    );
    assert_eq!(
        3,
        case.listed().await?.revisions.len(),
        "nothing is recorded"
    );
    assert!(case.staging_left().is_empty(), "{:?}", case.staging_left());
    Ok(())
}

#[tokio::test]
async fn an_imported_revision_applies_through_the_apply_route() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.saved().await?;
    let archive = zipped(&[("fhirconnect/ferrobridge_console.yml", MODEL)])?;
    assert_eq!(
        StatusCode::CREATED,
        case.import(archive, "application/zip").await?.status()
    );
    let response = case
        .send(
            Request::post("/operator/revisions/4/apply")
                .header(header::IF_MATCH, "\"3\"")
                .header("ferrobridge-reason", REASON)
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(!case.console("ferrobridge_console.context.yml").exists());
    Ok(())
}

#[tokio::test]
async fn an_import_that_is_no_mapping_archive_is_refused() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let good = zipped(&[("fhirconnect/ferrobridge_console.yml", MODEL)])?;
    for (archive, content_type) in [
        (good.clone(), "application/octet-stream"),
        (b"no zip".to_vec(), "application/zip"),
        (zipped(&[("elsewhere/a.yml", MODEL)])?, "application/zip"),
        (zipped(&[("fhirconnect/a.txt", MODEL)])?, "application/zip"),
    ] {
        let response = case.import(archive, content_type).await?;
        assert_eq!(StatusCode::BAD_REQUEST, response.status(), "{content_type}");
        let problem: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
        assert!(
            problem["type"]
                .as_str()
                .is_some_and(|kind| kind.ends_with("#bad-archive")),
            "{problem}"
        );
    }
    let unreasoned = case
        .send(
            Request::post("/operator/revisions/import")
                .header(header::CONTENT_TYPE, "application/zip")
                .body(Body::from(good))?,
        )
        .await?;
    assert_eq!(StatusCode::BAD_REQUEST, unreasoned.status());
    Ok(())
}
