// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access record of each facade interaction, as the Audit Record
//! Repository receives it over the ITI-20 ATX: FHIR Feed (ITI TF-2
//! §3.20.4.2): one BALP `AuditEvent` per access, naming the caller the token
//! verified (Regulation (EU) 2025/327 Annex II 3.2(a), (b)), the categories
//! the reached template is mapped to (c) and the CDR the data come from (e),
//! and an access refused before the CDR when its record has no place.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use ehds_logging::map::CategoryMap;
use ehds_logging::map::Declared;
use ehds_logging::map::Matching;
use ferrobridge_server::audit::AccessLog;
use ferrobridge_server::smart::Smart;
use ferrobridge_server::state::AppState;
use ferrohealth_health::registry::Registry;
use http::Request;
use http::StatusCode;
use http::header;
use jsonwebtoken::Algorithm;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::Issuer;
use super::SigningKey;
use super::authority;
use super::bearing;
use super::ec_key;
use super::get;
use super::mint;
use crate::facade::CONTAINER;
use crate::facade::EHR_ID;
use crate::facade::Harness;
use crate::facade::VERSION_ONE;
use crate::facade::VERSION_TWO;
use crate::facade::condition;
use crate::facade::harness;
use crate::facade::post_condition;
use crate::facade::raw;
use crate::facade::server_settings;
use crate::facade::stub::ECHOED;
use crate::facade::stub::Echo;
use crate::facade::stub::mount_create;
use crate::facade::stub::mount_echo;
use crate::facade::stub::mount_ehr;
use crate::facade::stub::mount_read;

/// The category the fixture template is mapped to in these cases.
const CATEGORY: &str = "Patient-Summaries";

/// One case: the facade, its issuer, the repository and the spool.
struct Recorded {
    /// The facade over the stub CDR.
    harness: Harness,
    /// The authorization server.
    issuer: Issuer,
    /// The key the tokens are signed with.
    key: SigningKey,
    /// The resource server over the issuer.
    authority: Arc<Smart>,
    /// The Audit Record Repository.
    arr: MockServer,
    /// The access-record lane.
    log: Arc<AccessLog>,
    /// The spool's directory, kept for the case.
    _spool: tempfile::TempDir,
}

/// Returns a case over `harness` whose spool holds at most `max_bytes`.
async fn recorded(harness: Harness, max_bytes: u64) -> Result<Recorded, Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let authority = authority(issuer.settings()).await;
    let arr = crate::audit::repository().await;
    let template = harness.composition()["archetype_details"]["template_id"]["value"]
        .as_str()
        .ok_or("the fixture composition names its template")?
        .to_owned();
    let categories = CategoryMap::declare_with(
        Matching::OpenEhr,
        &[],
        &BTreeMap::from([(template, Declared::Codes(vec![CATEGORY.to_owned()]))]),
        &BTreeMap::new(),
    )?;
    let spool = tempfile::tempdir()?;
    let log = crate::audit::access_log(
        &arr,
        spool.path(),
        harness.facade.client().base_url(),
        categories,
        max_bytes,
    )?;
    Ok(Recorded {
        harness,
        issuer,
        key,
        authority,
        arr,
        log,
        _spool: spool,
    })
}

impl Recorded {
    /// Returns the shipped router, guarded and recording.
    fn app(&self) -> Router {
        let state = AppState::with_health(Registry::default())
            .with_facade(Arc::clone(&self.harness.facade))
            .with_auth(Arc::clone(&self.authority))
            .with_access_log(Arc::clone(&self.log));
        ferrobridge_server::router(Arc::new(state), &server_settings())
    }

    /// Returns `request` under a token granting `scope`.
    fn as_caller(&self, request: Request<Body>, scope: &str) -> Request<Body> {
        bearing(request, &mint(&self.key, &self.issuer.claims(scope)))
    }

    /// Returns the `AuditEvent`s the repository received, waiting until it
    /// holds `count` of them.
    async fn records(&self, count: usize) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
        crate::audit::records(&self.arr, count).await
    }
}

/// Returns the `subtype` codes of `record`.
fn subtypes(record: &serde_json::Value) -> Vec<String> {
    record["subtype"]
        .as_array()
        .map(|codes| {
            codes
                .iter()
                .filter_map(|code| code["code"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Asserts what every record of a facade access names: the caller, the
/// category of the fixture template, the EHR and the CDR as the origin.
fn assert_access(record: &serde_json::Value, cdr: &str) {
    assert_eq!(
        Some("AuditEvent"),
        record["resourceType"].as_str(),
        "the repository receives an AuditEvent"
    );
    let text = record.to_string();
    assert!(
        text.contains("synthetic-client-sub"),
        "the token's sub names the person (Annex II 3.2(b)): {record}"
    );
    assert!(
        text.contains(CATEGORY),
        "the category of the template is named (3.2(c)): {record}"
    );
    assert!(text.contains(EHR_ID), "the EHR is named: {record}");
    assert!(
        text.contains(cdr),
        "the CDR is the origin (3.2(e)): {record}"
    );
}

#[tokio::test]
async fn a_create_is_recorded_as_a_create() -> Result<(), Box<dyn Error>> {
    let case = recorded(harness().await, 64 << 20).await?;
    mount_ehr(&case.harness.cdr).await;
    mount_create(&case.harness.cdr, VERSION_ONE, &case.harness.composition()).await;
    let response = raw(
        case.app(),
        case.as_caller(post_condition(&condition()), "system/Condition.c"),
    )
    .await?;
    assert_eq!(StatusCode::CREATED, response.status());
    let records = case.records(1).await?;
    assert_eq!(1, records.len(), "one access, one record");
    let record = &records[0];
    assert_eq!(Some("C"), record["action"].as_str(), "{record}");
    assert!(
        subtypes(record).contains(&String::from("create")),
        "{record}"
    );
    assert_eq!(Some("0"), record["outcome"].as_str(), "{record}");
    assert_access(record, &case.harness.cdr.uri());
    Ok(())
}

/// Creates the fixture Condition on the open facade, which records nothing,
/// and returns its id.
async fn created(case: &Recorded) -> Result<String, Box<dyn Error>> {
    mount_ehr(&case.harness.cdr).await;
    mount_create(&case.harness.cdr, VERSION_ONE, &case.harness.composition()).await;
    let response = raw(case.harness.app(), post_condition(&condition())).await?;
    assert_eq!(
        StatusCode::CREATED,
        response.status(),
        "the open facade creates the fixture"
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    Ok(body["id"].as_str().ok_or("an id")?.to_owned())
}

#[tokio::test]
async fn a_read_is_recorded_as_a_read_of_the_resource() -> Result<(), Box<dyn Error>> {
    let case = recorded(harness().await, 64 << 20).await?;
    let id = created(&case).await?;
    mount_read(&case.harness.cdr, VERSION_ONE, &case.harness.composition()).await;
    let response = raw(
        case.app(),
        case.as_caller(get(&format!("/fhir/Condition/{id}")), "system/Condition.r"),
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    let records = case.records(1).await?;
    let record = &records[0];
    assert_eq!(Some("R"), record["action"].as_str(), "{record}");
    assert!(subtypes(record).contains(&String::from("read")), "{record}");
    assert!(
        record.to_string().contains(&format!("Condition/{id}")),
        "the resource read is the data entity: {record}"
    );
    assert_access(record, &case.harness.cdr.uri());
    Ok(())
}

#[tokio::test]
async fn an_update_is_recorded_as_an_update() -> Result<(), Box<dyn Error>> {
    let case = recorded(harness().await, 64 << 20).await?;
    let id = created(&case).await?;
    mount_read(&case.harness.cdr, VERSION_ONE, &case.harness.composition()).await;
    Mock::given(matchers::method("PUT"))
        .and(matchers::path(format!(
            "/ehr/{EHR_ID}/composition/{CONTAINER}"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", format!("W/\"{VERSION_TWO}\""))
                .insert_header("Content-Type", "application/json")
                .set_body_string(case.harness.composition().to_string()),
        )
        .mount(&case.harness.cdr)
        .await;
    let mut resource = condition();
    resource["id"] = serde_json::json!(id);
    let request = Request::put(format!("/fhir/Condition/{id}"))
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .header(header::IF_MATCH, "W/\"1\"")
        .body(Body::from(resource.to_string()))?;
    let response = raw(case.app(), case.as_caller(request, "system/Condition.u")).await?;
    assert_eq!(StatusCode::OK, response.status());
    let records = case.records(1).await?;
    let record = &records[0];
    assert_eq!(Some("U"), record["action"].as_str(), "{record}");
    assert!(
        subtypes(record).contains(&String::from("update")),
        "{record}"
    );
    assert_access(record, &case.harness.cdr.uri());
    Ok(())
}

#[tokio::test]
async fn a_transaction_of_creates_is_recorded_as_a_create() -> Result<(), Box<dyn Error>> {
    let case = recorded(harness().await, 64 << 20).await?;
    mount_ehr(&case.harness.cdr).await;
    mount_echo(&case.harness.cdr, ECHOED, Echo::Sent).await;
    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "transaction",
        "entry": [
            { "fullUrl": "urn:uuid:0000-good", "resource": condition(),
              "request": { "method": "POST", "url": "Condition" } }
        ]
    });
    let request = Request::post("/fhir")
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(bundle.to_string()))?;
    let response = raw(case.app(), case.as_caller(request, "system/Condition.c")).await?;
    assert_eq!(StatusCode::OK, response.status());
    let records = case.records(1).await?;
    let record = &records[0];
    assert_eq!(Some("C"), record["action"].as_str(), "{record}");
    assert_access(record, &case.harness.cdr.uri());
    Ok(())
}

#[tokio::test]
async fn a_search_is_recorded_as_a_query() -> Result<(), Box<dyn Error>> {
    let harness =
        crate::facade::search_harness(ferrobridge_server::facade::search::Limits::default()).await;
    let case = recorded(harness, 64 << 20).await?;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/query/aql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "q": "synthetic",
            "rows": []
        })))
        .mount(&case.harness.cdr)
        .await;
    let response = raw(
        case.app(),
        case.as_caller(get("/fhir/Condition"), "system/Condition.s"),
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    let records = case.records(1).await?;
    let record = &records[0];
    assert_eq!(Some("E"), record["action"].as_str(), "{record}");
    assert!(
        subtypes(record).contains(&String::from("search")),
        "{record}"
    );
    assert!(
        record.to_string().contains("synthetic-client-sub"),
        "{record}"
    );
    Ok(())
}

#[tokio::test]
async fn an_access_whose_record_has_no_place_is_refused_before_the_cdr()
-> Result<(), Box<dyn Error>> {
    let case = recorded(harness().await, 1024).await?;
    mount_ehr(&case.harness.cdr).await;
    mount_create(&case.harness.cdr, VERSION_ONE, &case.harness.composition()).await;
    let response = raw(
        case.app(),
        case.as_caller(post_condition(&condition()), "system/Condition.c"),
    )
    .await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, response.status());
    assert_eq!(
        (0, 0),
        (
            case.harness.received("GET", "/ehr").await,
            case.harness.received("POST", "/ehr").await
        ),
        "an access with no record never reaches the CDR"
    );
    Ok(())
}

#[tokio::test]
async fn a_request_the_guard_refuses_leaves_no_record() -> Result<(), Box<dyn Error>> {
    let case = recorded(harness().await, 64 << 20).await?;
    let response = raw(case.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        case.arr
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "no access, no record"
    );
    Ok(())
}

#[tokio::test]
async fn a_machine_s_access_is_recorded_naming_its_client() -> Result<(), Box<dyn Error>> {
    let case = recorded(harness().await, 64 << 20).await?;
    mount_ehr(&case.harness.cdr).await;
    mount_create(&case.harness.cdr, VERSION_ONE, &case.harness.composition()).await;
    let token = mint(&case.key, &case.issuer.machine_claims("system/Condition.c"));

    let response = raw(case.app(), bearing(post_condition(&condition()), &token)).await?;

    assert_eq!(
        StatusCode::CREATED,
        response.status(),
        "the machine creates"
    );
    let records = case.records(1).await?;
    let record = records.first().ok_or("one record")?;
    // Regulation (EU) 2025/327 Annex II 3.2(a): the record names who accessed,
    // here the inbound client the bridge names as the originating system.
    assert!(
        record.to_string().contains(super::CLIENT),
        "the record names the machine's client: {record}"
    );
    Ok(())
}
