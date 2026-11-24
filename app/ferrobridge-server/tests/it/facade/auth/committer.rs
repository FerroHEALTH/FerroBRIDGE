// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The committer a header commit states to the CDR: none, whoever the
//! request authenticated, since the CDR writes who committed from the caller
//! it authenticated (ITS-REST 1.1.0 §openehr-version and
//! openehr-audit-details: "None of these headers are mandatory").

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

/// Asserts that `lines` state the change type and the bridge as the
/// committing system, and no committer.
fn assert_no_committer(lines: &[String]) {
    assert!(
        lines
            .iter()
            .any(|line| line == "change_type.code_string=\"249\""),
        "the change type travels: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line == "system_id=\"ferrobridge.test\""),
        "the bridge stays the committing system: {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("committer")),
        "the CDR writes who committed: {lines:?}"
    );
}

#[tokio::test]
async fn an_authenticated_callers_write_states_no_committer() -> Result<(), Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let app = secured(&harness, &authority(issuer.settings()).await);
    let token = mint(&key, &issuer.claims("system/Condition.c"));
    let response = raw(app, bearing(post_condition(&condition()), &token)).await?;
    assert_eq!(StatusCode::CREATED, response.status());
    assert_no_committer(&audit_lines(&harness.cdr).await?);
    Ok(())
}

#[tokio::test]
async fn a_write_on_an_open_surface_states_no_committer() -> Result<(), Box<dyn Error>> {
    let harness = harness().await;
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    let response = raw(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::CREATED, response.status());
    assert_no_committer(&audit_lines(&harness.cdr).await?);
    Ok(())
}
