// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The committer a write states to the CDR: the caller the request
//! authenticated, provisioned from the authentication runtime data as
//! ITS-REST 1.1.0 §openehr-version and openehr-audit-details recommends, and
//! the bridge itself for a write no caller made.

use std::error::Error;

use http::StatusCode;
use jsonwebtoken::Algorithm;

use super::Issuer;
use super::authority;
use super::bearing;
use super::ec_key;
use super::mint;
use super::secured;
use crate::facade::EHR_ID;
use crate::facade::VERSION_ONE;
use crate::facade::condition;
use crate::facade::harness;
use crate::facade::post_condition;
use crate::facade::raw;
use crate::facade::stub::mount_create;
use crate::facade::stub::mount_ehr;

/// The `openehr-audit-details` field lines the last composition commit
/// carried.
async fn audit_lines(cdr: &wiremock::MockServer) -> Result<Vec<String>, Box<dyn Error>> {
    let path = format!("/ehr/{EHR_ID}/composition");
    let requests = cdr.received_requests().await.unwrap_or_default();
    let commit = requests
        .iter()
        .rev()
        .find(|request| request.method.as_str() == "POST" && request.url.path() == path)
        .ok_or("the create sent a composition")?;
    Ok(commit
        .headers
        .get_all("openehr-audit-details")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_owned)
        .collect())
}

#[tokio::test]
async fn a_write_states_the_authenticated_caller_as_its_committer() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let app = secured(&harness, &authority(issuer.settings()).await);
    let token = mint(&key, &issuer.claims("system/Condition.c"));
    let response = raw(app, bearing(post_condition(&condition()), &token)).await?;
    assert_eq!(StatusCode::CREATED, response.status());
    let lines = audit_lines(&harness.cdr).await?;
    let committer = lines
        .iter()
        .find(|line| line.starts_with("committer."))
        .ok_or("the commit names a committer")?;
    assert!(
        committer.contains("committer.name=\"synthetic-client-sub\""),
        "the caller's sub names the committer: {committer}"
    );
    assert!(
        committer.contains(&format!(
            "committer.external_ref.namespace=\"{}\"",
            issuer.uri()
        )),
        "the issuer is the namespace: {committer}"
    );
    assert!(committer.contains("committer.external_ref.type=\"PARTY\""));
    assert!(
        lines
            .iter()
            .any(|line| line == "system_id=\"ferrobridge.test\""),
        "the bridge stays the committing system: {lines:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_write_on_an_open_surface_is_the_bridges() -> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let response = raw(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::CREATED, response.status());
    let lines = audit_lines(&harness.cdr).await?;
    assert!(
        lines
            .iter()
            .any(|line| line == "committer.name=\"ferrobridge.test\""),
        "no caller, so the bridge commits under its own name: {lines:?}"
    );
    Ok(())
}
