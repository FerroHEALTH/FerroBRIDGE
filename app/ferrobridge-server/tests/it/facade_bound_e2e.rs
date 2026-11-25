// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The facade against the reference CDR with authentication on, behind the
//! `FERROBRIDGE_E2E` gate.
//!
//! The bridge reaches the CDR as a Basic user bound to a professional
//! `PERSON` the CDR's SCIM endpoint provisioned, so the CDR writes the
//! committer of every commit from that party and refuses a stated committer
//! that says more. A header commit states none (ITS-REST 1.1.0
//! §openehr-version and openehr-audit-details: "None of these headers are
//! mandatory"); a CONTRIBUTION, whose `UpdateAudit` requires one, states the
//! placeholder naming the Basic user.

use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::Settings;
use ferrobridge_server::facade::ehr::Policy;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use ferrobridge_testkit::bound_cdr::{BASIC_USER, COMMITTER_ISSUER, PERSON_NAME, bound};
use ferrobridge_testkit::containers;
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::oidc::Provider;
use http::Request;
use http::StatusCode;
use http::header;

use crate::facade_e2e::{app, call, condition_coded, post_condition};

/// The fixture directory the case loads its mapping set from.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures");

/// Returns the committer name of every composition version the CDR holds,
/// read as the Basic user over AQL.
async fn committers(base_url: &str, password: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let answer: serde_json::Value = reqwest::Client::new()
        .post(format!("{base_url}/query/aql"))
        .basic_auth(BASIC_USER, Some(password))
        .json(&serde_json::json!({
            "q": "SELECT v/commit_audit/committer FROM EHR e CONTAINS VERSION v CONTAINS COMPOSITION c"
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    answer["rows"]
        .as_array()
        .ok_or_else(|| format!("the result set carries rows: {answer}"))?
        .iter()
        .map(|row| {
            row.pointer("/0/name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("the committer carries no name: {row}").into())
        })
        .collect()
}

#[tokio::test]
async fn a_facade_write_and_a_transaction_commit_under_the_committer_the_cdr_writes()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let provider = Provider::start().await?;
    let bound = bound(
        &provider,
        &[(fixtures::DIAGNOSE_TEMPLATE_ID, fixtures::DIAGNOSE_OPT)],
    )
    .await?;
    let cdr = bound.cdr();
    let client = CdrClient::new(
        &CdrConfig::new(cdr.base_url().parse()?)
            .with_basic(BASIC_USER, bound.password())
            .with_committer_issuer(Some(String::from(COMMITTER_ISSUER))),
    )?;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let programs = programs::compile_set(&set, &templates)?;
    let (_pix, subject) = crate::support::per_identifier(&[]).await;
    let facade = Arc::new(Facade::new(
        programs,
        Arc::new(MemoryStore::default()),
        client,
        Settings {
            base_url: String::from("http://ferrobridge.invalid/fhir"),
            ehr_policy: Policy::CreateOnFirstWrite,
            subject,
            system_id: String::from("ferrobridge.e2e"),
            language: String::from("en"),
            territory: String::from("GB"),
            ehr_status_system: None,
        },
    ));

    let created = call(app(&facade), post_condition()?).await?;
    assert_eq!(StatusCode::CREATED, created.0, "{}", created.1);
    let id = created.1["id"].as_str().ok_or("an id")?.to_owned();
    let read = call(
        app(&facade),
        Request::get(format!("/fhir/Condition/{id}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, read.0, "{}", read.1);

    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "transaction",
        "entry": [
            { "fullUrl": "urn:uuid:0000-bound-first",
              "resource": condition_coded("ferrobridge-bound-condition-1", "SYN-101")?,
              "request": { "method": "POST", "url": "Condition" } },
            { "fullUrl": "urn:uuid:0000-bound-second",
              "resource": condition_coded("ferrobridge-bound-condition-2", "SYN-102")?,
              "request": { "method": "POST", "url": "Condition" } }
        ]
    });
    let committed = call(
        app(&facade),
        Request::post("/fhir")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(bundle.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, committed.0, "{}", committed.1);

    assert_eq!(
        vec![String::from(PERSON_NAME); 3],
        committers(cdr.base_url(), bound.password()).await?,
        "every version names the professional the Basic user is bound to"
    );
    Ok(())
}
