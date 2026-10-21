// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The deletion sweep: a composition an earlier run committed and the query
//! no longer answers is read from the CDR, and only a `204`, the deleted
//! answer of ITS-REST 1.1.0 `composition_get` (`204_deleted_at_time`),
//! removes its rows. The CDR answers are wiremock stubs; the run cases also
//! need PostgreSQL and run only when `FERROBRIDGE_E2E=1`.

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::cdr::error::CdrError;
use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::etl::deletions::{DeletionPolicy, Presence, presence};
use ferrobridge_server::etl::report::RunReport;
use ferrobridge_server::etl::{RunError, RunOptions, run};
use ferrobridge_testkit::containers;
use ferrobridge_testkit::fixtures::{MINIMAL_EVALUATION_COMPOSITION, MINIMAL_EVALUATION_OPT};
use omop_cdm::writer::input::RunId;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_its::rest::client::RetryPolicy;
use serde_json::json;
use std::error::Error;
use std::time::Duration;
use wiremock::matchers::{body_string_contains, method, path, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::Behaviour;
use super::COMPOSITIONS_UIDS;
use super::EHR;
use super::Harness;
use super::Laboratory;
use super::composition_row;
use super::count;
use super::harness;
use super::settings;

/// Returns a client for `server`, with a retry budget short enough for a test.
fn quick_client(server: &MockServer) -> Result<CdrClient, Box<dyn Error>> {
    let config = CdrConfig::new(format!("{}/", server.uri()).parse()?)
        .with_timeout(Duration::from_secs(5))
        .with_retry(RetryPolicy {
            max_attempts: 2,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
        });
    Ok(CdrClient::new(&config)?)
}

/// Returns the path of the latest version of the composition `uid`.
fn latest(uid: &str) -> String {
    format!("/ehr/{EHR}/composition/{uid}")
}

/// Mounts the composition read of `uid` answering `status`, with the
/// synthetic composition as the body of a `200`.
async fn answer(server: &MockServer, uid: &str, status: u16) {
    let mut response = ResponseTemplate::new(status);
    if status == 200 {
        response = response
            .insert_header("Content-Type", "application/json")
            .insert_header("ETag", format!("\"{uid}::ferrobridge.test::1\""))
            .set_body_string(MINIMAL_EVALUATION_COMPOSITION);
    } else if status == 404 {
        response = response.set_body_json(json!({"message": "no such composition"}));
    }
    Mock::given(method("GET"))
        .and(path(latest(uid)))
        .and(query_param_is_missing("version_at_time"))
        .respond_with(response)
        .mount(server)
        .await;
}

/// Asks the stub whether the first composition is deleted.
async fn ask(server: &MockServer) -> Result<Result<Presence, CdrError>, Box<dyn Error>> {
    Ok(presence(
        &quick_client(server)?,
        &EhrId::new(EHR)?,
        &HierObjectId::new(COMPOSITIONS_UIDS[0])?,
    )
    .await)
}

#[tokio::test]
async fn a_204_on_the_latest_version_is_a_deletion() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    answer(&server, COMPOSITIONS_UIDS[0], 204).await;
    assert_eq!(Presence::Deleted, ask(&server).await??);
    Ok(())
}

#[tokio::test]
async fn a_200_on_the_latest_version_is_an_extant_composition() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    answer(&server, COMPOSITIONS_UIDS[0], 200).await;
    assert_eq!(Presence::Extant, ask(&server).await??);
    Ok(())
}

#[tokio::test]
async fn a_404_is_an_unknown_composition_and_no_deletion() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    answer(&server, COMPOSITIONS_UIDS[0], 404).await;
    assert_eq!(Presence::Unknown, ask(&server).await??);
    Ok(())
}

#[tokio::test]
async fn a_cdr_failure_is_a_typed_error_carrying_the_status() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    answer(&server, COMPOSITIONS_UIDS[0], 503).await;
    match ask(&server).await? {
        Err(CdrError::Client {
            upstream: Some(upstream),
            ..
        }) => assert_eq!(http::StatusCode::SERVICE_UNAVAILABLE, upstream.status()),
        other => return Err(format!("expected a CDR refusal, got {other:?}").into()),
    }
    Ok(())
}

/// A mapper that maps every composition.
const MAP_ALL: Laboratory = Laboratory {
    behaviours: [Behaviour::Map; 3],
    visit: false,
};

/// Remounts the stub so the composition query answers `rows`.
async fn query_answers(cdr: &MockServer, rows: Vec<serde_json::Value>) {
    cdr.reset().await;
    Mock::given(method("POST"))
        .and(path("/query/aql"))
        .and(body_string_contains("AS composition"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "rows": rows })))
        .mount(cdr)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/definition/template/adl1.4/ferrobridge.minimal_evaluation.v1",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/xml")
                .set_body_string(MINIMAL_EVALUATION_OPT),
        )
        .mount(cdr)
        .await;
}

/// Runs the ETL once under `policy`.
async fn run_under(
    harness: &mut Harness,
    policy: DeletionPolicy,
    run_id: &str,
) -> Result<Result<RunReport, RunError>, Box<dyn Error>> {
    let mut settings = settings(false)?;
    settings.deletions = policy;
    Ok(run(
        &settings,
        &RunOptions::default(),
        &quick_client(&harness.cdr)?,
        &mut harness.writer,
        &MAP_ALL,
        &RunId::new(run_id)?,
    )
    .await)
}

/// Loads the three compositions, then makes the query answer the first two.
async fn loaded_then_third_missing() -> Result<Harness, Box<dyn Error>> {
    let mut harness = harness().await?;
    let first = run_under(&mut harness, DeletionPolicy::Remove, "run-1").await??;
    assert_eq!(3, first.compositions.committed, "{first}");
    assert_eq!(0, first.deletions.checked, "every watermark was read");
    query_answers(&harness.cdr, (0..2).map(composition_row).collect()).await;
    Ok(harness)
}

/// Returns the side table rows of the third composition.
async fn third_rows(harness: &Harness) -> Result<i64, Box<dyn Error>> {
    count(
        &harness.postgres,
        &format!(
            "SELECT count(*) FROM ferrobridge.record WHERE versioned_object_uid = '{}'",
            COMPOSITIONS_UIDS[2]
        ),
    )
    .await
}

#[tokio::test]
async fn a_composition_the_cdr_reports_deleted_loses_its_rows_and_a_rerun_changes_nothing()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut harness = loaded_then_third_missing().await?;
    answer(&harness.cdr, COMPOSITIONS_UIDS[2], 204).await;
    let second = run_under(&mut harness, DeletionPolicy::Remove, "run-2").await??;
    assert_eq!(1, second.deletions.checked, "{second}");
    assert_eq!(vec![COMPOSITIONS_UIDS[2]], second.deletions.removed);
    assert_eq!(Some(&3), second.deletions.rows.get("measurement"));
    assert_eq!(Some(&4), second.deletions.rows.get("fact_relationship"));
    assert_eq!(0, third_rows(&harness).await?);
    assert_eq!(
        6,
        count(&harness.postgres, "SELECT count(*) FROM cdm.measurement").await?
    );
    assert_eq!(
        8,
        count(
            &harness.postgres,
            "SELECT count(*) FROM cdm.fact_relationship"
        )
        .await?
    );
    assert_eq!(
        2,
        count(
            &harness.postgres,
            "SELECT count(*) FROM ferrobridge.watermark"
        )
        .await?
    );
    let before = super::dump(&harness.postgres).await?;

    let third = run_under(&mut harness, DeletionPolicy::Remove, "run-3").await??;
    assert_eq!(0, third.deletions.checked, "no watermark is left to check");
    assert_eq!(third.deletions.removed, Vec::<String>::new());
    assert_eq!(before, super::dump(&harness.postgres).await?);
    Ok(())
}

#[tokio::test]
async fn a_composition_the_cdr_still_holds_keeps_its_rows() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut harness = loaded_then_third_missing().await?;
    answer(&harness.cdr, COMPOSITIONS_UIDS[2], 200).await;
    let second = run_under(&mut harness, DeletionPolicy::Remove, "run-2").await??;
    assert_eq!(1, second.deletions.checked, "{second}");
    assert_eq!(1, second.deletions.extant);
    assert_eq!(second.deletions.removed, Vec::<String>::new());
    assert_eq!(3, third_rows(&harness).await?);
    Ok(())
}

#[tokio::test]
async fn a_composition_the_cdr_does_not_know_keeps_its_rows_and_is_listed()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut harness = loaded_then_third_missing().await?;
    answer(&harness.cdr, COMPOSITIONS_UIDS[2], 404).await;
    let second = run_under(&mut harness, DeletionPolicy::Remove, "run-2").await??;
    assert_eq!(vec![COMPOSITIONS_UIDS[2]], second.deletions.unknown);
    assert_eq!(second.deletions.removed, Vec::<String>::new());
    assert_eq!(3, third_rows(&harness).await?);
    Ok(())
}

#[tokio::test]
async fn the_report_policy_lists_a_deletion_and_keeps_its_rows() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut harness = loaded_then_third_missing().await?;
    answer(&harness.cdr, COMPOSITIONS_UIDS[2], 204).await;
    let second = run_under(&mut harness, DeletionPolicy::Report, "run-2").await??;
    assert_eq!(vec![COMPOSITIONS_UIDS[2]], second.deletions.kept);
    assert_eq!(second.deletions.removed, Vec::<String>::new());
    assert_eq!(3, third_rows(&harness).await?);
    assert!(
        second.to_string().contains(COMPOSITIONS_UIDS[2]),
        "{second}"
    );
    Ok(())
}

#[tokio::test]
async fn a_cdr_that_fails_the_check_ends_the_run_and_removes_nothing() -> Result<(), Box<dyn Error>>
{
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut harness = loaded_then_third_missing().await?;
    answer(&harness.cdr, COMPOSITIONS_UIDS[2], 503).await;
    match run_under(&mut harness, DeletionPolicy::Remove, "run-2").await? {
        Err(RunError::Deletion { composition, .. }) => {
            assert_eq!(COMPOSITIONS_UIDS[2], composition);
        }
        other => return Err(format!("expected a deletion check error, got {other:?}").into()),
    }
    assert_eq!(3, third_rows(&harness).await?);
    Ok(())
}

#[tokio::test]
async fn deleting_every_composition_of_a_visit_removes_the_visit_and_its_observation_period()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let mut harness = harness().await?;
    let visiting = Laboratory {
        behaviours: [Behaviour::Map; 3],
        visit: true,
    };
    let first = run(
        &settings(true)?,
        &RunOptions::default(),
        &quick_client(&harness.cdr)?,
        &mut harness.writer,
        &visiting,
        &RunId::new("run-1")?,
    )
    .await?;
    assert_eq!(1, first.derived.visit_occurrence, "{first}");
    assert_eq!(1, first.derived.observation_period, "{first}");

    query_answers(&harness.cdr, Vec::new()).await;
    Mock::given(method("POST"))
        .and(path("/query/aql"))
        .and(body_string_contains("AS visit_source"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "rows": [] })))
        .mount(&harness.cdr)
        .await;
    for uid in COMPOSITIONS_UIDS {
        answer(&harness.cdr, uid, 204).await;
    }
    let second = run(
        &settings(true)?,
        &RunOptions::default(),
        &quick_client(&harness.cdr)?,
        &mut harness.writer,
        &visiting,
        &RunId::new("run-2")?,
    )
    .await?;
    assert_eq!(3, second.deletions.removed.len(), "{second}");
    assert_eq!(1, second.derived.visit_occurrence_removed, "{second}");
    assert_eq!(0, second.derived.observation_period, "{second}");
    for table in [
        "cdm.visit_occurrence",
        "ferrobridge.visit",
        "cdm.observation_period",
        "cdm.measurement",
    ] {
        assert_eq!(
            0,
            count(&harness.postgres, &format!("SELECT count(*) FROM {table}")).await?,
            "{table} is empty"
        );
    }
    Ok(())
}
