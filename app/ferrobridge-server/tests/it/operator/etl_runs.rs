// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET /operator/etl/runs` and `GET /operator/etl/runs/{run_id}`: the OMOP
//! ETL runs the bridge schema keeps, `404` without a `[cdm]`, and the
//! `run-history-unavailable` problem while the CDM database does not answer.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use ferrobridge_operator_api::etl::{Run, RunKind, Runs};
use ferrobridge_server::config::Process;
use ferrobridge_server::etl::history::{self, Store};
use ferrobridge_server::etl::report::{RefusalEntry, RunReport};
use ferrobridge_testkit::containers;
use http::StatusCode;
use omop_cdm::ddl::SchemaName;
use omop_cdm::writer::CdmWriter;
use omop_cdm::writer::input::PersonPolicy;

use super::{Case, OPERATOR, bytes};

/// Returns the `[cdm]` settings over the database at `url`.
fn cdm(url: &str) -> Result<ferrobridge_server::config::CdmSettings, Box<dyn Error>> {
    let settings = ferrobridge_server::config::Config::from_sources(
        Some(&format!("[cdm]\nurl = \"{url}\"\n")),
        &BTreeMap::new(),
    )?
    .resolve(Process::Server)?;
    Ok(settings.cdm.ok_or("the [cdm] section resolves")?)
}

/// A synthetic report: three compositions read, one refused.
fn report(run_id: &str) -> RunReport {
    let mut report = RunReport::new(run_id);
    report.compositions.read = 3;
    report.compositions.committed = 2;
    report.compositions.refused = 1;
    report.rows.insert(String::from("measurement"), 4);
    report.refusals.push(RefusalEntry {
        composition: Some(String::from("8849182c-82ad-4088-a07f-48ead4180515")),
        version: None,
        mapping: None,
        table: None,
        column: None,
        element: None,
        reason: String::from("the composition names no template"),
    });
    report
        .deletions
        .removed
        .push(String::from("7d44b88c-4199-4bad-97dc-d78268e01398"));
    report
}

#[test]
fn a_report_becomes_its_run_document_and_summary() -> Result<(), Box<dyn Error>> {
    let run = history::document(
        &report("0b6c5a3e-1d3f-4c7e-9a43-000000000001"),
        RunKind::Compositions,
        "2026-11-03T09:00:00Z".parse()?,
        "2026-11-03T09:01:30Z".parse()?,
    );
    assert_eq!("2026-11-03T09:00:00Z", run.started);
    assert_eq!("2026-11-03T09:01:30Z", run.finished);
    assert_eq!(1, run.compositions.refused);
    assert_eq!(
        Some("the composition names no template"),
        run.refusals.first().map(|refusal| refusal.reason.as_str())
    );
    let summary = history::summary(&run);
    assert_eq!((4, 1, 1), (summary.rows, summary.refusals, summary.removed));
    Ok(())
}

#[tokio::test]
async fn a_bridge_with_no_cdm_answers_not_found() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    for path in [
        "/operator/etl/runs",
        "/operator/etl/runs/0b6c5a3e-1d3f-4c7e-9a43-000000000001",
    ] {
        let response = case.get(path, Some(OPERATOR)).await?;
        assert_eq!(StatusCode::NOT_FOUND, response.status(), "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn a_cdm_database_that_does_not_answer_is_a_problem() -> Result<(), Box<dyn Error>> {
    let closed = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let store = Store::new(&cdm(&format!(
        "postgres://ferrobridge@127.0.0.1:{closed}/ferrobridge?sslmode=disable"
    ))?)?;
    let case = Case::keeping(Arc::new(store)).await?;
    let response = case.get("/operator/etl/runs", Some(OPERATOR)).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, response.status());
    let problem: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(
        Some("https://ferrobridge.eu/docs/operate/problems.html#run-history-unavailable"),
        problem["type"].as_str()
    );
    let response = case
        .get("/operator/etl/runs/no%20such%20run", Some(OPERATOR))
        .await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        response.status(),
        "an id that is no run id never reaches the database"
    );
    Ok(())
}

#[tokio::test]
async fn a_recorded_run_is_listed_and_read_back_whole() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let mut writer = CdmWriter::connect(
        postgres.url(),
        SchemaName::new("cdm")?,
        SchemaName::new("ferrobridge")?,
        PersonPolicy::CreateOnFirstSight,
    )
    .await?;
    writer.init().await?;
    let older = history::document(
        &report("0b6c5a3e-1d3f-4c7e-9a43-000000000001"),
        RunKind::Compositions,
        "2026-11-03T09:00:00Z".parse()?,
        "2026-11-03T09:01:30Z".parse()?,
    );
    let newer = history::document(
        &RunReport::new("0b6c5a3e-1d3f-4c7e-9a43-000000000002"),
        RunKind::Fhir,
        "2026-11-03T10:00:00Z".parse()?,
        "2026-11-03T10:00:05Z".parse()?,
    );
    history::record(&writer, &older).await?;
    history::record(&writer, &newer).await?;
    let case = Case::keeping(Arc::new(Store::new(&cdm(postgres.url())?)?)).await?;

    let response = case.get("/operator/etl/runs", Some(OPERATOR)).await?;
    assert_eq!(StatusCode::OK, response.status());
    let runs: Runs = serde_json::from_slice(&bytes(response).await?)?;
    let ids: Vec<&str> = runs.runs.iter().map(|run| run.id.as_str()).collect();
    assert_eq!(
        vec![newer.id.as_str(), older.id.as_str()],
        ids,
        "newest first"
    );

    let response = case
        .get(&format!("/operator/etl/runs/{}", older.id), Some(OPERATOR))
        .await?;
    assert_eq!(StatusCode::OK, response.status());
    let read: Run = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(older, read);

    let response = case
        .get(
            "/operator/etl/runs/0b6c5a3e-0000-0000-0000-000000000000",
            Some(OPERATOR),
        )
        .await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}
