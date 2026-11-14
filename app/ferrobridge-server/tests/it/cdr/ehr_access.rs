// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `EHR_ACCESS` contract: the read the CDR serves as an extension shaped
//! on `ehr_status_get_at_time` (`GET /ehr/{ehr_id}/ehr_access`, the version
//! as the `ETag`), the CONTRIBUTION that commits a new version
//! (`ehr-codegen.openapi.yaml`, `contribution_create`), and the
//! `ACCESS_POLICY` read of the CDR's administrative extension. One case per
//! status each call reads, plus the request-side assertions of the commit.

use std::error::Error;

use ferrobridge_server::cdr::ehr_access::AccessError;
use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::commit::Change;
use ferrobridge_server::facade::commit::Committer;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_rm::draft::access_control::access_control::access_directive_settings::AccessDirectiveSettings;
use openehr_term::codes::VersionLifecycleState;
use serde_json::Value;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;

use super::support;

/// The synthetic EHR.
const EHR: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The `EHR_ACCESS` version the stub holds.
const VERSION_1: &str = "3a4c2b1e-0000-4000-8000-000000000002::ferrobridge.test::1";

/// The contribution the commit answers.
const CONTRIBUTION: &str = "4b5d3c2f-0000-4000-8000-000000000003";

/// A synthetic `EHR_ACCESS` carrying one whole-EHR permit.
pub(crate) const EHR_ACCESS_JSON: &str = r#"{
  "_type": "EHR_ACCESS",
  "name": {"_type": "DV_TEXT", "value": "EHR access"},
  "archetype_node_id": "openEHR-EHR-EHR_ACCESS.access_directives.v1.0.0",
  "settings": {
    "_type": "ACCESS_DIRECTIVE_SETTINGS",
    "default_access": {"_type": "DV_CODED_TEXT", "value": "restricted", "defining_code": {"_type": "CODE_PHRASE", "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "openehr_access_control"}, "code_string": "restricted"}},
    "directives": [{
      "_type": "ACCESS_DIRECTIVE",
      "name": {"_type": "DV_TEXT", "value": "consent directive"},
      "archetype_node_id": "id5",
      "uid": {"_type": "HIER_OBJECT_ID", "value": "9c1d2e3f-0000-4000-8000-000000000004"},
      "kind": {"_type": "DV_CODED_TEXT", "value": "consent", "defining_code": {"_type": "CODE_PHRASE", "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "openehr_access_control"}, "code_string": "consent"}},
      "effect": {"_type": "DV_CODED_TEXT", "value": "permit", "defining_code": {"_type": "CODE_PHRASE", "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "openehr_access_control"}, "code_string": "permit"}},
      "emergency_override": false
    }]
  }
}"#;

/// The CDR's extensions root, under the stub, as the reference CDR serves it.
const EXTENSIONS: &str = "/ferroehr/rest/extensions/v1";

/// The path of the synthetic EHR's `EHR_ACCESS`.
fn access_path() -> String {
    format!("{EXTENSIONS}/ehr/{EHR}/ehr_access")
}

/// The extensions root of `server`.
fn extensions(server: &MockServer) -> Result<url::Url, Box<dyn Error>> {
    Ok(url::Url::parse(&format!("{}{EXTENSIONS}", server.uri()))?)
}

/// Mounts the read answering `response`.
async fn mount_read(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(access_path()))
        .respond_with(response)
        .mount(server)
        .await;
}

/// Reads the synthetic EHR's `EHR_ACCESS`.
async fn read(
    server: &MockServer,
) -> Result<Result<ferrobridge_server::cdr::ehr_access::CurrentAccess, AccessError>, Box<dyn Error>>
{
    Ok(support::client(server)?
        .ehr_access(&extensions(server)?, &EhrId::new(EHR)?)
        .await)
}

#[tokio::test]
async fn a_200_reads_the_ehr_access_and_its_version() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_read(
        &server,
        ResponseTemplate::new(200)
            .insert_header("ETag", format!("\"{VERSION_1}\"").as_str())
            .set_body_raw(EHR_ACCESS_JSON, "application/json"),
    )
    .await;
    let current = read(&server).await??;
    assert_eq!(current.version.value(), VERSION_1);
    let settings = current
        .access
        .settings
        .as_ref()
        .map(AccessDirectiveSettings::try_from)
        .transpose()?
        .ok_or("the EHR_ACCESS carries settings")?;
    assert_eq!(settings.directives.map(|all| all.len()), Some(1));
    Ok(())
}

#[tokio::test]
async fn a_200_without_an_etag_is_refused() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_read(
        &server,
        ResponseTemplate::new(200).set_body_raw(EHR_ACCESS_JSON, "application/json"),
    )
    .await;
    assert!(matches!(read(&server).await?, Err(AccessError::Cdr(_))));
    Ok(())
}

#[tokio::test]
async fn a_200_that_is_no_ehr_access_is_refused() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_read(
        &server,
        ResponseTemplate::new(200)
            .insert_header("ETag", format!("\"{VERSION_1}\"").as_str())
            .set_body_raw(r#"{"_type": "EHR_STATUS"}"#, "application/json"),
    )
    .await;
    assert!(matches!(
        read(&server).await?,
        Err(AccessError::Body { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn a_404_on_the_read_is_not_found() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_read(&server, ResponseTemplate::new(404)).await;
    assert!(matches!(
        read(&server).await?,
        Err(AccessError::NotFound { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn the_read_asks_for_json_under_the_extensions_root() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(access_path()))
        .and(header("accept", "application/json"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", format!("\"{VERSION_1}\"").as_str())
                .set_body_raw(EHR_ACCESS_JSON, "application/json"),
        )
        .mount(&server)
        .await;
    read(&server).await??;
    Ok(())
}

#[tokio::test]
async fn a_403_on_the_read_is_denied() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_read(&server, ResponseTemplate::new(403)).await;
    assert!(matches!(
        read(&server).await?,
        Err(AccessError::Denied { .. })
    ));
    Ok(())
}

/// Mounts the commit answering `response`.
async fn mount_commit(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(format!("/v1/ehr/{EHR}/contribution")))
        .and(header("prefer", "return=minimal"))
        .and(header("content-type", "application/json"))
        .respond_with(response)
        .mount(server)
        .await;
}

/// Commits the synthetic `EHR_ACCESS` over [`VERSION_1`].
async fn commit(
    server: &MockServer,
) -> Result<Result<ferrobridge_server::cdr::ids::ContributionUid, AccessError>, Box<dyn Error>> {
    let access = openehr_its::json::from_canonical_json(EHR_ACCESS_JSON)?;
    Ok(support::client(server)?
        .commit_ehr_access(
            &EhrId::new(EHR)?,
            &ObjectVersionId::new(VERSION_1)?,
            &access,
            ferrobridge_server::facade::commit::lifecycle_state(VersionLifecycleState::Complete),
            ferrobridge_server::facade::commit::audit(
                Change::Modification,
                "ferrobridge.test",
                &Committer::Bridge,
            ),
        )
        .await)
}

#[tokio::test]
async fn the_commit_is_one_ehr_access_version_over_the_one_read() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_commit(
        &server,
        ResponseTemplate::new(201).insert_header("ETag", format!("\"{CONTRIBUTION}\"").as_str()),
    )
    .await;
    let contribution = commit(&server).await??;
    assert_eq!(contribution.as_str(), CONTRIBUTION);
    let requests = server.received_requests().await.ok_or("recorded")?;
    let sent: Value = serde_json::from_slice(&requests.first().ok_or("one request")?.body)?;
    let version = sent
        .pointer("/versions/0")
        .ok_or("the contribution carries one version")?;
    assert_eq!(
        version.pointer("/preceding_version_uid/value"),
        Some(&Value::from(VERSION_1))
    );
    assert_eq!(
        version.pointer("/data/_type"),
        Some(&Value::from("EHR_ACCESS"))
    );
    assert_eq!(
        version.pointer("/data/settings/_type"),
        Some(&Value::from("ACCESS_DIRECTIVE_SETTINGS"))
    );
    assert_eq!(
        version.pointer("/commit_audit/change_type/defining_code/code_string"),
        Some(&Value::from("251"))
    );
    assert_eq!(
        version.pointer("/lifecycle_state/defining_code/code_string"),
        Some(&Value::from("532"))
    );
    Ok(())
}

#[tokio::test]
async fn a_204_names_the_contribution_in_its_etag() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_commit(
        &server,
        ResponseTemplate::new(204).insert_header("ETag", format!("\"{CONTRIBUTION}\"").as_str()),
    )
    .await;
    assert_eq!(commit(&server).await??.as_str(), CONTRIBUTION);
    Ok(())
}

#[tokio::test]
async fn a_409_on_the_commit_is_a_conflict() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_commit(&server, ResponseTemplate::new(409)).await;
    assert!(matches!(
        commit(&server).await?,
        Err(AccessError::Conflict { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn a_422_on_the_commit_is_refused_with_the_cdr_words() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_commit(
        &server,
        ResponseTemplate::new(422).set_body_raw(
            r#"{"message":"directive 0 names a party the demographic store does not hold"}"#,
            "application/json",
        ),
    )
    .await;
    match commit(&server).await? {
        Err(AccessError::Refused { upstream }) => {
            assert!(upstream.body().contains("demographic store"));
        }
        other => return Err(format!("expected a refusal, got {other:?}").into()),
    }
    Ok(())
}

#[tokio::test]
async fn a_400_on_the_commit_is_refused() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_commit(&server, ResponseTemplate::new(400)).await;
    assert!(matches!(
        commit(&server).await?,
        Err(AccessError::Refused { .. })
    ));
    Ok(())
}

#[tokio::test]
async fn a_500_on_the_commit_is_undocumented() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_commit(&server, ResponseTemplate::new(500)).await;
    assert!(matches!(
        commit(&server).await?,
        Err(AccessError::Undocumented { .. })
    ));
    Ok(())
}

/// A synthetic `ACCESS_POLICY` with an empty catalogue.
const POLICY_JSON: &str = r#"{
  "_type": "ACCESS_POLICY",
  "name": {"_type": "DV_TEXT", "value": "access policy"},
  "archetype_node_id": "openEHR-EHR-ACCESS_POLICY.access_policy.v1.0.0",
  "controller": {"_type": "PARTY_REF", "namespace": "demographic", "type": "ORGANISATION", "id": {"_type": "HIER_OBJECT_ID", "value": "5c6e4d3a-0000-4000-8000-000000000005"}},
  "care_relationship_required": false,
  "purpose_required": false
}"#;

#[tokio::test]
async fn the_policy_is_read_under_the_extensions_root_and_a_404_is_none()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{EXTENSIONS}/admin/access_policy")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(POLICY_JSON, "application/json"))
        .mount(&server)
        .await;
    let root = extensions(&server)?;
    let policy = support::client(&server)?.access_policy(&root).await?;
    assert!(policy.is_some_and(|policy| !policy.purpose_required));
    let empty = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("{EXTENSIONS}/admin/access_policy")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&empty)
        .await;
    let root = extensions(&empty)?;
    assert!(
        support::client(&empty)?
            .access_policy(&root)
            .await?
            .is_none()
    );
    Ok(())
}
