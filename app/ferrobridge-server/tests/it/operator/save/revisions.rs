// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping-set revisions: each save, reload and apply recorded as a
//! whole set of console files, the diff against the running revision, and
//! the apply of an earlier one under `If-Match` with its reason. No
//! specification governs the operator surface: our own design.

use std::collections::BTreeMap;
use std::error::Error;

use axum::body::Body;
use ferrobridge_operator_api::revisions::{FileChange, Origin, Revision, RevisionDiff, Revisions};
use ferrobridge_server::operator::revisions::{Entry, Store};
use http::{Request, StatusCode, header};

use super::{BROKEN, CONTEXT, Case, MODEL, REASON};
use crate::operator::bytes;

/// The wire path of the model the cases save.
const MODEL_PATH: &str = "fhirconnect/ferrobridge_console.yml";

/// The wire path of the context the cases save.
const CONTEXT_PATH: &str = "fhirconnect/ferrobridge_console.context.yml";

impl Case {
    /// Returns the revision listing.
    async fn revisions(&self) -> Result<Revisions, Box<dyn Error>> {
        let response = self
            .send(Request::get("/operator/revisions").body(Body::empty())?)
            .await?;
        assert_eq!(
            StatusCode::OK,
            response.status(),
            "the revisions route answers"
        );
        Ok(serde_json::from_slice(&bytes(response).await?)?)
    }

    /// Applies the revision `id` naming `running` in `If-Match`.
    async fn apply(
        &self,
        id: &str,
        running: Option<&str>,
        reason: Option<&str>,
    ) -> Result<http::Response<Body>, Box<dyn Error>> {
        let mut request = Request::post(format!("/operator/revisions/{id}/apply"));
        if let Some(running) = running {
            request = request.header(header::IF_MATCH, format!("\"{running}\""));
        }
        if let Some(reason) = reason {
            request = request.header("ferrobridge-reason", reason);
        }
        self.send(request.body(Body::empty())?).await
    }

    /// Saves the model and then the context, three revisions in all.
    async fn saved_twice(&self) -> Result<(), Box<dyn Error>> {
        assert_eq!(
            StatusCode::OK,
            self.create("ferrobridge_console.yml", MODEL)
                .await?
                .status(),
            "the model saves"
        );
        assert_eq!(
            StatusCode::OK,
            self.create("ferrobridge_console.context.yml", CONTEXT)
                .await?
                .status(),
            "the context saves"
        );
        Ok(())
    }

    /// Returns the revision store under the console tree.
    fn store(&self) -> Store {
        Store::new(&self.tree.path().join("console"))
    }
}

#[tokio::test]
async fn each_save_records_the_whole_set_as_a_revision() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.saved_twice().await?;
    let listed = case.revisions().await?;
    assert_eq!(Some(String::from("3")), listed.running);
    let origins = listed
        .revisions
        .iter()
        .map(|revision| (revision.id.as_str(), revision.origin, revision.files))
        .collect::<Vec<_>>();
    assert_eq!(
        vec![
            ("3", Origin::Save, 2),
            ("2", Origin::Save, 1),
            ("1", Origin::Baseline, 0)
        ],
        origins
    );
    let newest = &listed.revisions[0];
    assert_eq!("synthetic-client-sub", newest.author);
    assert_eq!(REASON, newest.reason);
    assert_eq!(Some(String::from("2")), newest.parent);
    let response = case
        .send(Request::get("/operator/revisions/3").body(Body::empty())?)
        .await?;
    assert_eq!(StatusCode::OK, response.status());
    let revision: Revision = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(
        vec![CONTEXT_PATH, MODEL_PATH],
        revision
            .files
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        Some(&super::super::digest(CONTEXT.as_bytes())),
        revision.files.get(CONTEXT_PATH)
    );
    Ok(())
}

#[tokio::test]
async fn the_provisioned_files_are_in_no_revision() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.saved_twice().await?;
    let revision = case.store().get("3")?.ok_or("revision 3")?;
    assert!(
        revision
            .files
            .keys()
            .all(|path| path.starts_with("fhirconnect/ferrobridge_console")),
        "{:?}",
        revision.files
    );
    Ok(())
}

#[tokio::test]
async fn the_diff_names_each_changed_file_with_its_lines() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.saved_twice().await?;
    let response = case
        .send(Request::get("/operator/revisions/2/diff").body(Body::empty())?)
        .await?;
    assert_eq!(StatusCode::OK, response.status());
    let diff: RevisionDiff = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(Some(String::from("3")), diff.from);
    assert_eq!("2", diff.to);
    assert_eq!(1, diff.files.len(), "the unchanged model is left out");
    let file = &diff.files[0];
    assert_eq!(CONTEXT_PATH, file.path);
    assert_eq!(FileChange::Removed, file.change);
    assert_eq!(None, file.to_sha256);
    assert!(file.unified.contains("-type: context"), "{}", file.unified);
    assert!(
        file.unified
            .starts_with(&format!("--- a/{CONTEXT_PATH}\n+++ b/{CONTEXT_PATH}\n")),
        "{}",
        file.unified
    );
    Ok(())
}

#[tokio::test]
async fn an_apply_rolls_back_to_an_earlier_revision_and_records_it() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.saved_twice().await?;
    assert!(
        case.declared_types()
            .await?
            .contains(&String::from("Observation"))
    );
    let response = case
        .apply("2", Some("3"), Some("synthetic rollback"))
        .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        Some("\"4\""),
        response
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
    );
    assert!(!case.console("ferrobridge_console.context.yml").exists());
    assert_eq!(
        MODEL,
        std::fs::read_to_string(case.console("ferrobridge_console.yml"))?
    );
    assert!(
        !case
            .declared_types()
            .await?
            .contains(&String::from("Observation"))
    );
    let revision = case.store().get("4")?.ok_or("revision 4")?;
    assert_eq!(Origin::Apply, revision.origin);
    assert_eq!(Some(String::from("2")), revision.source);
    assert_eq!(Some(String::from("3")), revision.parent);
    assert_eq!("synthetic rollback", revision.reason);
    assert_eq!(Some(String::from("4")), case.revisions().await?.running);
    assert!(case.staging_left().is_empty(), "{:?}", case.staging_left());
    Ok(())
}

#[tokio::test]
async fn an_apply_over_a_stale_running_revision_answers_412() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.saved_twice().await?;
    let response = case.apply("2", Some("2"), Some(REASON)).await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, response.status());
    assert!(case.console("ferrobridge_console.context.yml").exists());
    Ok(())
}

#[tokio::test]
async fn an_apply_without_a_precondition_or_a_reason_is_refused() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.saved_twice().await?;
    assert_eq!(
        StatusCode::PRECONDITION_REQUIRED,
        case.apply("2", None, Some(REASON)).await?.status()
    );
    assert_eq!(
        StatusCode::BAD_REQUEST,
        case.apply("2", Some("3"), None).await?.status()
    );
    assert_eq!(
        StatusCode::NOT_FOUND,
        case.apply("9", Some("3"), Some(REASON)).await?.status()
    );
    assert_eq!(
        StatusCode::NOT_FOUND,
        case.send(Request::get("/operator/revisions/x").body(Body::empty())?)
            .await?
            .status()
    );
    Ok(())
}

#[tokio::test]
async fn an_apply_that_does_not_compile_is_refused_and_changes_nothing()
-> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.saved_twice().await?;
    let broken = case.store().record(
        Entry {
            origin: Origin::Import,
            author: String::from("synthetic-client-sub"),
            reason: String::from(REASON),
            applied: false,
            source: None,
        },
        &BTreeMap::from([
            (String::from(MODEL_PATH), BROKEN.as_bytes().to_vec()),
            (String::from(CONTEXT_PATH), CONTEXT.as_bytes().to_vec()),
        ]),
    )?;
    assert_eq!("4", broken.id);
    assert_eq!(Some(String::from("3")), case.revisions().await?.running);
    let response = case.apply("4", Some("3"), Some(REASON)).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, response.status());
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(false, report["clean"]);
    assert!(
        report["diagnostics"]
            .as_array()
            .ok_or("diagnostics")?
            .iter()
            .any(|diagnostic| diagnostic["file"] == MODEL_PATH),
        "{report}"
    );
    assert_eq!(
        MODEL,
        std::fs::read_to_string(case.console("ferrobridge_console.yml"))?
    );
    assert_eq!(Some(String::from("3")), case.revisions().await?.running);
    assert!(case.staging_left().is_empty(), "{:?}", case.staging_left());
    Ok(())
}

#[tokio::test]
async fn a_reload_of_files_changed_on_disk_records_a_revision() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    case.saved_twice().await?;
    std::fs::remove_file(case.console("ferrobridge_console.context.yml"))?;
    let response = case
        .send(Request::post("/operator/reload").body(Body::empty())?)
        .await?;
    assert_eq!(StatusCode::OK, response.status());
    let listed = case.revisions().await?;
    assert_eq!(Some(String::from("4")), listed.running);
    assert_eq!(Origin::Reload, listed.revisions[0].origin);
    assert_eq!("-", listed.revisions[0].author);
    let again = case
        .send(Request::post("/operator/reload").body(Body::empty())?)
        .await?;
    assert_eq!(StatusCode::OK, again.status());
    assert_eq!(
        4,
        case.revisions().await?.revisions.len(),
        "no change, no revision"
    );
    Ok(())
}

#[test]
fn the_apply_audit_line_names_the_caller_the_revisions_and_the_reason() -> Result<(), Box<dyn Error>>
{
    let logs = crate::support::Logs::default();
    let capture = crate::support::subscriber(
        ferrohealth_telemetry::section::Rendering::Json,
        "info",
        false,
        logs.clone(),
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    tracing::subscriber::with_default(capture, || {
        runtime.block_on(async {
            let case = Case::new().await?;
            case.saved_twice().await?;
            let response = case.apply("2", Some("3"), Some(REASON)).await?;
            assert_eq!(StatusCode::OK, response.status());
            Ok::<(), Box<dyn Error>>(())
        })
    })?;
    let text = logs.text();
    let applies: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|line| line["target"] == "ferrobridge::audit" && line["action"] == "apply")
        .collect();
    assert_eq!(1, applies.len(), "{text}");
    let apply = &applies[0];
    assert_eq!("synthetic-client-sub", apply["subject"]);
    assert_eq!("2", apply["revision"]);
    assert_eq!("3", apply["running"]);
    assert_eq!("4", apply["recorded"]);
    assert_eq!(REASON, apply["reason"]);
    assert_eq!(200, apply["status"]);
    assert!(!text.contains("problemDiagnose"), "{text}");
    Ok(())
}
