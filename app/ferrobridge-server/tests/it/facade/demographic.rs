// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The four party resources on the wire, over a `wiremock` Demographic API
//! (ITS-REST 1.1.0 Demographic API, DEVELOPMENT).
//!
//! `Patient`, `Practitioner`, `PractitionerRole` and `Organization` are served
//! from the party contexts under `tests/it/fixtures_demographic`, over the
//! testkit's synthetic party templates. The id is the party's
//! `versioned_object_uid` and `meta.versionId` its version tree id; every
//! status the Demographic API documents reaches the caller through the
//! facade's status table. No specification governs a party context: our own
//! design.

use std::error::Error as StdError;
use std::sync::Arc;

use axum::body::Body;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use ferrobridge_server::facade::programs::LoadError;
use http::Request;
use http::StatusCode;
use http::header;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::app_over;
use super::call;
use super::handle;
use super::settings;

/// The demographic fixture mapping set.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures_demographic");

/// The object id of the synthetic party every case reads or writes.
pub(super) const UID: &str = "a1b2c3d4-0000-4000-8000-000000000001";

/// The creating system of every version.
const SYSTEM: &str = "synthetic.example";

/// Returns the `OBJECT_VERSION_ID` of version `tree` of the party.
pub(super) fn version(tree: u32) -> String {
    format!("{UID}::{SYSTEM}::{tree}")
}

/// Returns the canonical `PARTY` of `class` and `template` the CDR holds,
/// with `name` on its identity and `id` on its details identifier.
fn party(class: &str, concept: &str, template: &str, name: &str, tree: u32) -> serde_json::Value {
    serde_json::json!({
        "_type": class,
        "name": {"_type": "DV_TEXT", "value": "Synthetic party"},
        "archetype_node_id": format!("openEHR-DEMOGRAPHIC-{class}.{concept}.v1"),
        "uid": {"_type": "OBJECT_VERSION_ID", "value": version(tree)},
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": {"_type": "ARCHETYPE_ID", "value": format!("openEHR-DEMOGRAPHIC-{class}.{concept}.v1")},
            "template_id": {"_type": "TEMPLATE_ID", "value": template},
            "rm_version": "1.1.0"
        },
        "identities": [{
            "_type": "PARTY_IDENTITY",
            "name": {"_type": "DV_TEXT", "value": "Identity"},
            "archetype_node_id": "at0003",
            "details": {
                "_type": "ITEM_TREE",
                "name": {"_type": "DV_TEXT", "value": "Identity details"},
                "archetype_node_id": "at0004",
                "items": [{
                    "_type": "ELEMENT",
                    "name": {"_type": "DV_TEXT", "value": "Name"},
                    "archetype_node_id": "at0005",
                    "value": {"_type": "DV_TEXT", "value": name}
                }]
            }
        }]
    })
}

/// Returns the synthetic `PERSON` at version `tree`.
fn person(tree: u32) -> serde_json::Value {
    party(
        "PERSON",
        "ferrobridge_person",
        "ferrobridge.person.v1",
        "Synthetic Person",
        tree,
    )
}

/// Returns a synthetic `Patient` with a name.
pub(super) fn patient() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Patient",
        "name": [{"text": "Synthetic Person"}]
    })
}

/// Returns the stub CDR and a facade over the demographic fixture set, with
/// `[cdr] demographic` on.
async fn harness() -> Result<(MockServer, Arc<Facade>), Box<dyn StdError>> {
    harness_with(settings()).await
}

/// Returns the stub CDR and a facade over the demographic fixture set that
/// runs with `facade_settings`, with `[cdr] demographic` on.
pub(super) async fn harness_with(
    facade_settings: ferrobridge_server::facade::Settings,
) -> Result<(MockServer, Arc<Facade>), Box<dyn StdError>> {
    let cdr = MockServer::start().await;
    for (id, opt) in [
        (
            "ferrobridge.person.v1",
            ferrobridge_testkit::fixtures::PERSON_OPT,
        ),
        (
            "ferrobridge.organisation.v1",
            ferrobridge_testkit::fixtures::ORGANISATION_OPT,
        ),
        (
            "ferrobridge.role.v1",
            ferrobridge_testkit::fixtures::ROLE_OPT,
        ),
    ] {
        Mock::given(matchers::method("GET"))
            .and(matchers::path(format!("/definition/template/adl1.4/{id}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "application/xml")
                    .set_body_string(opt),
            )
            .mount(&cdr)
            .await;
    }
    let client = demographic_client(&cdr, true)?;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    programs::check_parties(&compiled, true)?;
    let store = Arc::new(MemoryStore::new());
    let facade = Facade::new(compiled, handle(&store), client, facade_settings);
    Ok((cdr, Arc::new(facade)))
}

/// Returns a client for `cdr` with `[cdr] demographic` set as `on`.
fn demographic_client(cdr: &MockServer, on: bool) -> Result<CdrClient, Box<dyn StdError>> {
    let base = format!("{}/", cdr.uri()).parse()?;
    Ok(CdrClient::new(&CdrConfig::new(base).with_demographic(on))?)
}

/// Mounts `GET /demographic/person/{uid}` answering `200` with `body`.
async fn mount_person(cdr: &MockServer, uid: &str, body: &serde_json::Value, tree: u32) {
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!("/demographic/person/{uid}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .insert_header("ETag", format!("\"{}\"", version(tree)))
                .set_body_json(body),
        )
        .mount(cdr)
        .await;
}

/// Returns an openEHR error answer with `status`, carrying `etag` when given.
fn refusal(status: u16, etag: Option<&str>) -> ResponseTemplate {
    let answer = ResponseTemplate::new(status)
        .insert_header("Content-Type", "application/json")
        .set_body_json(serde_json::json!({"message": "synthetic refusal", "validationErrors": ["synthetic detail"]}));
    match etag {
        Some(tag) => answer.insert_header("ETag", tag),
        None => answer,
    }
}

/// Sends a request with a FHIR JSON body.
pub(super) fn with_body(
    method: http::Method,
    uri: &str,
    body: &serde_json::Value,
) -> Result<Request<Body>, Box<dyn StdError>> {
    Ok(Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(body.to_string()))?)
}

#[tokio::test]
async fn the_statement_declares_the_party_types_as_development() -> Result<(), Box<dyn StdError>> {
    let (_cdr, facade) = harness().await?;
    let (status, body) = call(
        app_over(&facade),
        Request::get("/fhir/metadata").body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let resources = body["rest"][0]["resource"]
        .as_array()
        .ok_or("the statement lists resources")?;
    for wanted in [
        "Organization",
        "Patient",
        "Practitioner",
        "PractitionerRole",
    ] {
        let entry = resources
            .iter()
            .find(|entry| entry["type"] == wanted)
            .ok_or(format!("{wanted} is declared"))?;
        let codes: Vec<&str> = entry["interaction"]
            .as_array()
            .ok_or("interactions")?
            .iter()
            .filter_map(|interaction| interaction["code"].as_str())
            .collect();
        assert_eq!(
            codes,
            [
                "read",
                "vread",
                "history-instance",
                "create",
                "update",
                "delete"
            ],
            "{wanted}"
        );
        assert!(
            entry.get("searchParam").is_none(),
            "{wanted} offers no search"
        );
        let documentation = entry["documentation"].as_str().unwrap_or_default();
        assert!(
            documentation.contains("ITS-REST 1.1.0 Demographic API, DEVELOPMENT"),
            "{wanted}: {documentation}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_patient_creates_a_person() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/demographic/person"))
        .respond_with(
            ResponseTemplate::new(204).insert_header("ETag", format!("\"{}\"", version(1))),
        )
        .mount(&cdr)
        .await;
    let (status, body) = call(
        app_over(&facade),
        with_body(http::Method::POST, "/fhir/Patient", &patient())?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    assert_eq!(body["id"], UID);
    assert_eq!(body["meta"]["versionId"], "1");
    assert_eq!(body["name"][0]["text"], "Synthetic Person");
    let requests = cdr.received_requests().await.unwrap_or_default();
    let sent = requests
        .iter()
        .find(|request| request.method.as_str() == "POST")
        .ok_or("the CDR received the person")?;
    let sent: serde_json::Value = serde_json::from_slice(&sent.body)?;
    assert_eq!(sent["_type"], "PERSON");
    assert!(sent["identities"].is_array(), "{sent}");
    assert_eq!(
        sent["archetype_details"]["template_id"]["value"],
        "ferrobridge.person.v1"
    );
    assert!(sent.get("category").is_none(), "{sent}");
    Ok(())
}

#[tokio::test]
async fn a_patient_reads_from_its_person() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    mount_person(&cdr, UID, &person(2), 2).await;
    let (status, body) = call(
        app_over(&facade),
        Request::get(format!("/fhir/Patient/{UID}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(body["resourceType"], "Patient");
    assert_eq!(body["id"], UID);
    assert_eq!(body["meta"]["versionId"], "2");
    assert_eq!(body["name"][0]["text"], "Synthetic Person");
    Ok(())
}

#[tokio::test]
async fn one_person_reads_as_a_practitioner_under_the_same_id() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    mount_person(&cdr, UID, &person(1), 1).await;
    let (status, body) = call(
        app_over(&facade),
        Request::get(format!("/fhir/Practitioner/{UID}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(body["resourceType"], "Practitioner");
    assert_eq!(body["id"], UID);
    Ok(())
}

#[tokio::test]
async fn a_patient_vreads_one_version() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    mount_person(&cdr, UID, &person(2), 2).await;
    mount_person(&cdr, &version(1), &person(1), 1).await;
    let (status, body) = call(
        app_over(&facade),
        Request::get(format!("/fhir/Patient/{UID}/_history/1")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(body["meta"]["versionId"], "1");
    Ok(())
}

#[tokio::test]
async fn a_patient_lists_its_history() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    mount_person(&cdr, &version(1), &person(1), 1).await;
    mount_person(&cdr, &version(2), &person(2), 2).await;
    let audit = serde_json::json!({
        "_type": "AUDIT_DETAILS",
        "system_id": SYSTEM,
        "time_committed": {"_type": "DV_DATE_TIME", "value": "2026-10-19T08:00:00Z"},
        "change_type": {"_type": "DV_CODED_TEXT", "value": "creation", "defining_code": {"_type": "CODE_PHRASE", "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "openehr"}, "code_string": "249"}},
        "committer": {"_type": "PARTY_SELF"}
    });
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "/demographic/versioned_party/{UID}/revision_history"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .set_body_json(serde_json::json!({
                    "_type": "REVISION_HISTORY",
                    "items": [
                        {"_type": "REVISION_HISTORY_ITEM", "version_id": {"_type": "OBJECT_VERSION_ID", "value": version(1)}, "audits": [audit]},
                        {"_type": "REVISION_HISTORY_ITEM", "version_id": {"_type": "OBJECT_VERSION_ID", "value": version(2)}, "audits": [audit]}
                    ]
                })),
        )
        .mount(&cdr)
        .await;
    let (status, body) = call(
        app_over(&facade),
        Request::get(format!("/fhir/Patient/{UID}/_history")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(body["type"], "history");
    assert_eq!(body["total"], 2);
    assert_eq!(body["entry"][0]["resource"]["meta"]["versionId"], "2");
    assert_eq!(body["entry"][0]["request"]["method"], "PUT");
    assert_eq!(body["entry"][1]["resource"]["meta"]["versionId"], "1");
    assert_eq!(body["entry"][1]["request"]["method"], "POST");
    Ok(())
}

#[tokio::test]
async fn a_patient_updates_over_the_version_if_match_names() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    mount_person(&cdr, UID, &person(1), 1).await;
    Mock::given(matchers::method("PUT"))
        .and(matchers::path(format!("/demographic/person/{UID}")))
        .and(matchers::header(
            "If-Match",
            format!("\"{}\"", version(1)).as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(204).insert_header("ETag", format!("\"{}\"", version(2))),
        )
        .mount(&cdr)
        .await;
    let mut resource = patient();
    resource["id"] = serde_json::Value::from(UID);
    let mut request = with_body(
        http::Method::PUT,
        &format!("/fhir/Patient/{UID}"),
        &resource,
    )?;
    request
        .headers_mut()
        .insert(header::IF_MATCH, http::HeaderValue::from_static("W/\"1\""));
    let (status, body) = call(app_over(&facade), request).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(body["meta"]["versionId"], "2");
    Ok(())
}

#[tokio::test]
async fn a_patient_deletes_its_latest_version() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    mount_person(&cdr, UID, &person(1), 1).await;
    Mock::given(matchers::method("DELETE"))
        .and(matchers::path(format!(
            "/demographic/person/{}",
            version(1)
        )))
        .respond_with(ResponseTemplate::new(204))
        .mount(&cdr)
        .await;
    let (status, body) = call(
        app_over(&facade),
        Request::delete(format!("/fhir/Patient/{UID}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::NO_CONTENT, status, "{body}");
    // RM Common IM, Logical Deletion: a delete commits a version whose lifecycle state is
    // `deleted` (523), and its audit's change type is `deleted` (523).
    let requests = cdr.received_requests().await.unwrap_or_default();
    let sent = requests
        .iter()
        .find(|request| request.method.as_str() == "DELETE")
        .ok_or("the CDR received the delete")?;
    let version = sent
        .headers
        .get("openehr-version")
        .and_then(|value| value.to_str().ok())
        .ok_or("the delete carries openehr-version")?;
    assert!(
        version.contains("lifecycle_state.code_string=\"523\""),
        "{version}"
    );
    let audit = sent
        .headers
        .get("openehr-audit-details")
        .and_then(|value| value.to_str().ok())
        .ok_or("the delete carries openehr-audit-details")?;
    assert!(audit.contains("change_type.code_string=\"523\""), "{audit}");
    Ok(())
}

#[tokio::test]
async fn a_party_of_another_template_is_not_found() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    let other = party(
        "PERSON",
        "ferrobridge_person",
        "another.template.v1",
        "Synthetic Person",
        1,
    );
    mount_person(&cdr, UID, &other, 1).await;
    let (status, body) = call(
        app_over(&facade),
        Request::get(format!("/fhir/Patient/{UID}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn a_demographic_400_is_a_500() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/demographic/person"))
        .respond_with(refusal(400, None))
        .mount(&cdr)
        .await;
    let (status, body) = call(
        app_over(&facade),
        with_body(http::Method::POST, "/fhir/Patient", &patient())?,
    )
    .await?;
    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, status, "{body}");
    assert_diagnoses(&body);
    Ok(())
}

#[tokio::test]
async fn a_demographic_404_is_a_404() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!("/demographic/person/{UID}")))
        .respond_with(refusal(404, None))
        .mount(&cdr)
        .await;
    let (status, body) = call(
        app_over(&facade),
        Request::get(format!("/fhir/Patient/{UID}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    assert_diagnoses(&body);
    Ok(())
}

#[tokio::test]
async fn a_demographic_409_is_a_409() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    mount_person(&cdr, UID, &person(1), 1).await;
    Mock::given(matchers::method("DELETE"))
        .and(matchers::path(format!(
            "/demographic/person/{}",
            version(1)
        )))
        .respond_with(refusal(409, Some(&format!("\"{}\"", version(2)))))
        .mount(&cdr)
        .await;
    let (status, body) = call(
        app_over(&facade),
        Request::delete(format!("/fhir/Patient/{UID}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{body}");
    assert_diagnoses(&body);
    Ok(())
}

#[tokio::test]
async fn a_demographic_412_is_a_412() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    mount_person(&cdr, UID, &person(2), 2).await;
    Mock::given(matchers::method("PUT"))
        .and(matchers::path(format!("/demographic/person/{UID}")))
        .respond_with(refusal(412, Some(&format!("\"{}\"", version(2)))))
        .mount(&cdr)
        .await;
    let mut resource = patient();
    resource["id"] = serde_json::Value::from(UID);
    let mut request = with_body(
        http::Method::PUT,
        &format!("/fhir/Patient/{UID}"),
        &resource,
    )?;
    request
        .headers_mut()
        .insert(header::IF_MATCH, http::HeaderValue::from_static("W/\"1\""));
    let (status, body) = call(app_over(&facade), request).await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, status, "{body}");
    assert_diagnoses(&body);
    Ok(())
}

#[tokio::test]
async fn a_demographic_422_is_a_422() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness().await?;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/demographic/person"))
        .respond_with(refusal(422, None))
        .mount(&cdr)
        .await;
    let (status, body) = call(
        app_over(&facade),
        with_body(http::Method::POST, "/fhir/Patient", &patient())?,
    )
    .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    assert_diagnoses(&body);
    Ok(())
}

/// Asserts that the CDR's error body travels in the issue's diagnostics.
fn assert_diagnoses(body: &serde_json::Value) {
    let diagnostics = body["issue"][0]["diagnostics"].as_str().unwrap_or_default();
    assert!(
        diagnostics.contains("synthetic refusal") && diagnostics.contains("synthetic detail"),
        "{body}"
    );
}

#[tokio::test]
async fn a_party_context_with_the_gate_off_does_not_load() -> Result<(), Box<dyn StdError>> {
    let (_cdr, facade) = harness().await?;
    let refused = programs::check_parties(&facade.programs(), false);
    assert!(
        matches!(refused, Err(LoadError::DemographicOff { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_type_no_party_context_serves_has_no_delete_or_history() -> Result<(), Box<dyn StdError>>
{
    let facade = super::harness().await.facade;
    let deleted = call(
        app_over(&facade),
        Request::delete(format!("/fhir/Patient/{UID}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, deleted.0, "{}", deleted.1);
    assert_eq!(
        deleted.1["issue"][0]["code"], "not-supported",
        "{}",
        deleted.1
    );
    let history = call(
        app_over(&facade),
        Request::get(format!("/fhir/Patient/{UID}/_history")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, history.0, "{}", history.1);
    assert_eq!(
        history.1["issue"][0]["code"], "not-supported",
        "{}",
        history.1
    );
    Ok(())
}
