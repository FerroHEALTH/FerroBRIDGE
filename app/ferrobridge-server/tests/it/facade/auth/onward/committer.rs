// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The committer a write under the onward lane states: none in a header
//! commit and the placeholder the CDR discards in a CONTRIBUTION, both for a
//! person's exchanged token, and the client a machine authenticates as.

use std::error::Error;

use http::StatusCode;
use http::header;
use wiremock::Request;

use super::super::authority;
use super::super::bearing;
use super::super::condition_of;
use super::super::mint;
use super::super::secured;
use super::CDR_AUDIENCE;
use super::Grant;
use super::OWN_TOKEN;
use super::World;
use super::assert_every_call_under;
use super::create_and_read;
use super::encoded;
use crate::facade::post_condition;
use crate::facade::raw;
use crate::facade::stub::ECHOED;
use crate::facade::stub::Echo;
use crate::facade::stub::mount_echo;
use crate::facade::stub::mount_ehr;
use crate::facade::transaction_of;

/// Returns the committer the CDR reads from an onward token issued for the
/// person of `World::person_claims`: the placeholder the CDR discards, a
/// `PARTY_IDENTIFIED` whose only member is one `DV_IDENTIFIER` of the token's
/// `iss` and `sub`.
fn placeholder(world: &World) -> serde_json::Value {
    serde_json::json!({
        "_type": "PARTY_IDENTIFIED",
        "identifiers": [{
            "_type": "DV_IDENTIFIER",
            "issuer": world.issuer.uri(),
            "id": "synthetic-client-sub"
        }]
    })
}

#[tokio::test]
async fn a_header_commit_under_an_onward_token_states_no_committer() -> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Standard, "").await?;
    let issued = mint(&world.key, &world.issued_claims(Grant::Standard, 300));
    world
        .mount_token_endpoint(Grant::Standard, &issued, 300)
        .await;
    world.mount_cdr(&condition_of("p-1")).await;
    let token = mint(&world.key, &world.person_claims());

    create_and_read(&world, &token).await?;

    let commits: Vec<Request> = world
        .harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|request| {
            request.method.as_str() == "POST" && request.url.path().ends_with("/composition")
        })
        .collect();
    assert_eq!(1, commits.len(), "one composition commit");
    for commit in &commits {
        // ITS-REST 1.1.0: "None of these headers are mandatory", and `committer` is a
        // required member of `openehr-audit-details`, so the CDR derives it from the token.
        assert!(
            commit.headers.get("openehr-audit-details").is_none(),
            "a header commit under an onward token states no audit"
        );
        assert!(
            commit.headers.get("openehr-version").is_some(),
            "the lifecycle still travels"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_contribution_under_an_onward_token_carries_the_placeholder_committer()
-> Result<(), Box<dyn Error>> {
    let world = World::start(Grant::Standard, "").await?;
    let issued = mint(&world.key, &world.issued_claims(Grant::Standard, 300));
    world
        .mount_token_endpoint(Grant::Standard, &issued, 300)
        .await;
    mount_ehr(&world.harness.cdr).await;
    mount_echo(&world.harness.cdr, ECHOED, Echo::Sent).await;
    let token = mint(&world.key, &world.person_claims());
    let authority = authority(world.trusted.clone()).await;
    let bundle = transaction_of(&[("urn:uuid:0000-onward", condition_of("p-1"))]);
    let request = http::Request::post("/fhir")
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(axum::body::Body::from(bundle.to_string()))?;

    let response = raw(
        secured(&world.harness, &authority),
        bearing(request, &token),
    )
    .await?;

    assert_eq!(StatusCode::OK, response.status());
    let sent = world
        .harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|request| request.url.path().ends_with("/contribution"))
        .ok_or("the contribution reached the CDR")?;
    let body: serde_json::Value = serde_json::from_slice(&sent.body)?;
    let expected = placeholder(&world);
    // ITS-REST `schemas/common/UpdateAudit.yaml` requires `committer` in the
    // CONTRIBUTION's `audit` and `schemas/ehr/UpdateVersion.yaml` a
    // `commit_audit` per member: each carries the placeholder in its exact form.
    assert_eq!(expected, body["audit"]["committer"], "{body}");
    let versions = body["versions"].as_array().ok_or("the versions")?;
    assert!(!versions.is_empty(), "{body}");
    for version in versions {
        assert_eq!(expected, version["commit_audit"]["committer"], "{body}");
    }
    assert_every_call_under(&world.cdr_calls().await, &issued);
    Ok(())
}

/// Creates a Condition as a machine under `grant` and returns the composition
/// the CDR received and the client credentials request the bridge sent.
async fn machine_create(grant: Grant) -> Result<(serde_json::Value, String), Box<dyn Error>> {
    let world = World::start(grant, "").await?;
    world.mount_own_token().await;
    world.mount_cdr(&condition_of("p-1")).await;
    let token = mint(
        &world.key,
        &world.issuer.machine_claims("system/Condition.cru"),
    );
    let authority = authority(world.trusted.clone()).await;
    let created = raw(
        secured(&world.harness, &authority),
        bearing(post_condition(&condition_of("p-1")), &token),
    )
    .await?;
    assert_eq!(StatusCode::CREATED, created.status(), "the create");
    assert!(
        world.onward_requests().await.is_empty(),
        "a machine was exchanged"
    );
    assert_every_call_under(&world.cdr_calls().await, &OWN_TOKEN);
    let commit = world
        .harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|request| {
            request.method.as_str() == "POST" && request.url.path().ends_with("/composition")
        })
        .ok_or("the composition reached the CDR")?;
    let own = world
        .token_requests()
        .await
        .into_iter()
        .find(|body| body.contains("grant_type=client_credentials"))
        .ok_or("the bridge fetched its own token")?;
    Ok((serde_json::from_slice(&commit.body)?, own))
}

#[tokio::test]
async fn a_machine_s_composition_names_its_client_as_the_originating_system()
-> Result<(), Box<dyn Error>> {
    for grant in [Grant::Standard, Grant::Entra] {
        let (composition, own) = machine_create(grant).await?;
        // RM FEEDER_AUDIT: `originating_system_audit` is the audit "from the originating
        // system", `feeder_system_audit` that of the feeder "if different".
        assert_eq!(
            Some(super::super::CLIENT),
            composition
                .pointer("/feeder_audit/originating_system_audit/system_id")
                .and_then(serde_json::Value::as_str),
            "{composition}"
        );
        assert_eq!(
            Some("ferrobridge.test"),
            composition
                .pointer("/feeder_audit/feeder_system_audit/system_id")
                .and_then(serde_json::Value::as_str),
            "{composition}"
        );
        // RFC 8707 §2: the bridge's own token is asked for the CDR alone.
        assert!(
            own.contains(&format!("resource={}", encoded(CDR_AUDIENCE))),
            "{own}"
        );
    }
    Ok(())
}
