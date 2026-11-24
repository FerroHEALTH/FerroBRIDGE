// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART App State on the facade (SMART App Launch 2.2.0, Persisting App
//! State): the page's API examples replayed in order, each refusal the page
//! names, and the `CapabilityStatement` that declares the endpoint.

use std::error::Error;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::owned::OwnedStore;
use ferrobridge_server::facade::programs::Programs;
use http::Request;
use http::StatusCode;
use http::header;
use serde_json::Value;
use serde_json::json;
use wiremock::MockServer;

use super::BASE_URL;
use super::Harness;
use super::app_over;
use super::call;
use super::client;
use super::handle;
use super::raw;
use super::settings;

/// The state code of the page's Example 2 and 3.
const KEYS: &str = "https://myapp.example.org|encrypted-phr-access-keys";

/// One case: the store's directory and the harness over it.
pub(super) struct Case {
    /// The state store the resources live in, and its PostgreSQL.
    _state: (
        ferrobridge_testkit::containers::Postgres,
        ferrobridge_server::store::Store,
    ),
    /// The harness, its facade serving App State.
    pub(super) harness: Harness,
}

impl Case {
    /// Returns a facade with no program over a state store, `None` unless the
    /// harness admits a PostgreSQL, serving App State.
    pub(super) async fn new() -> Result<Option<Self>, Box<dyn Error>> {
        let Some(state) = crate::store::sealed_store().await? else {
            return Ok(None);
        };
        let owned = OwnedStore::open(&state.1);
        let cdr = MockServer::start().await;
        let store = Arc::new(MemoryStore::default());
        let facade = Facade::new(
            Programs::default(),
            handle(&store),
            client(&cdr),
            settings(),
        )
        .with_app_state(Some(Arc::new(owned)));
        Ok(Some(Self {
            _state: state,
            harness: Harness {
                cdr,
                store,
                facade: Arc::new(facade),
            },
        }))
    }

    /// Returns the open router over the facade.
    fn app(&self) -> Router {
        app_over(&self.harness.facade)
    }
}

/// Returns `reference` under the facade's base.
pub(super) fn at_base(reference: &str) -> String {
    format!("{BASE_URL}/{reference}")
}

/// Returns the page's Example 1 body, the user preferences of a practitioner.
pub(super) fn preferences() -> Value {
    json!({
        "resourceType": "Basic",
        "subject": {"reference": at_base("Practitioner/123")},
        "code": {"coding": [{"system": "https://myapp.example.org", "code": "display-preferences"}]},
        "extension": [{
            "url": "https://myapp.example.org/display-preferences-v2.0.1",
            "valueString": "{\"defaultView\":\"problem-list\",\"colorblindMode\":\"D\",\"resultsPerPage\":150}"
        }]
    })
}

/// Returns the page's Example 2 body, the access keys of a patient.
pub(super) fn keys(value: &str) -> Value {
    json!({
        "resourceType": "Basic",
        "subject": {"reference": at_base("Patient/123")},
        "code": {"coding": [{"system": "https://myapp.example.org", "code": "encrypted-phr-access-keys"}]},
        "extension": [{"url": "https://myapp.example.org/encrypted-phr-access-keys", "valueString": value}]
    })
}

/// Returns a request of `method` on `path` carrying `body` as FHIR JSON.
pub(super) fn with_body(method: &str, path: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(body.to_string()))
        .expect("a legal request")
}

/// Returns `request` carrying `If-Match: tag`.
pub(super) fn matching(mut request: Request<Body>, tag: &str) -> Request<Body> {
    request
        .headers_mut()
        .insert(header::IF_MATCH, tag.parse().expect("a legal header value"));
    request
}

/// Returns a `GET` of `path`.
pub(super) fn get(path: &str) -> Request<Body> {
    Request::get(path)
        .body(Body::empty())
        .expect("a legal request")
}

#[tokio::test]
async fn the_page_examples_create_query_update_and_delete_in_order() -> Result<(), Box<dyn Error>> {
    let Some(case) = Case::new().await? else {
        return Ok(());
    };
    // NOTE: SMART App Launch 2.2.0, Persisting App State, §API Examples, Example 1.
    let response = raw(case.app(), with_body("POST", "/fhir/Basic", &preferences())).await?;
    assert_eq!(StatusCode::CREATED, response.status());
    assert_eq!(
        Some("W/\"1\""),
        response
            .headers()
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
    );
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or("a Location")?
        .to_owned();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let created: Value = serde_json::from_slice(&bytes)?;
    let preferences_id = created["id"].as_str().ok_or("an id")?.to_owned();
    assert_eq!(
        at_base(&format!("Basic/{preferences_id}/_history/1")),
        location
    );
    assert_eq!(Some("1"), created["meta"]["versionId"].as_str());
    assert_eq!(preferences()["extension"], created["extension"]);
    assert_eq!(preferences()["code"], created["code"]);
    assert_eq!(preferences()["subject"], created["subject"]);

    let query = format!(
        "/fhir/Basic?subject={}&code=https://myapp.example.org%7Cdisplay-preferences",
        at_base("Practitioner/123")
    );
    let (status, bundle) = call(case.app(), get(&query)).await?;
    assert_eq!(StatusCode::OK, status, "{bundle}");
    assert_eq!(Some("searchset"), bundle["type"].as_str());
    assert_eq!(Some(1), bundle["total"].as_u64());
    assert_eq!(created, bundle["entry"][0]["resource"]);

    // NOTE: Example 2, the access keys of a patient.
    let (status, keys_created) = call(
        case.app(),
        with_body("POST", "/fhir/Basic", &keys("eyJhbGc-a")),
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{keys_created}");
    let id = keys_created["id"].as_str().ok_or("an id")?.to_owned();

    // NOTE: Example 3, the update under If-Match.
    let mut update = keys("eyJhbGc-b");
    update["id"] = json!(id);
    let path = format!("/fhir/Basic/{id}");
    let (status, updated) = call(
        case.app(),
        matching(with_body("PUT", &path, &update), "W/\"1\""),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{updated}");
    assert_eq!(Some("2"), updated["meta"]["versionId"].as_str());
    assert_eq!(
        Some("eyJhbGc-b"),
        updated["extension"][0]["valueString"].as_str()
    );

    // NOTE: §Updates, a stale version is 412.
    let (status, _) = call(
        case.app(),
        matching(with_body("PUT", &path, &update), "W/\"1\""),
    )
    .await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, status);

    // NOTE: Example 4, the delete under If-Match, then every later request about the id fails.
    let delete = || {
        Request::delete(path.as_str())
            .body(Body::empty())
            .expect("a legal request")
    };
    let response = raw(case.app(), matching(delete(), "W/\"2\"")).await?;
    assert_eq!(StatusCode::NO_CONTENT, response.status());
    let (status, _) = call(case.app(), matching(delete(), "W/\"3\"")).await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, status);
    let (status, _) = call(
        case.app(),
        matching(with_body("PUT", &path, &update), "W/\"3\""),
    )
    .await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, status);
    let (status, bundle) = call(
        case.app(),
        get(&format!("/fhir/Basic?code={}", KEYS.replace('|', "%7C"))),
    )
    .await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(Some(0), bundle["total"].as_u64(), "{bundle}");
    Ok(())
}

#[tokio::test]
async fn global_state_is_found_with_subject_missing() -> Result<(), Box<dyn Error>> {
    let Some(case) = Case::new().await? else {
        return Ok(());
    };
    let mut global = preferences();
    global.as_object_mut().ok_or("an object")?.remove("subject");
    let (status, _) = call(case.app(), with_body("POST", "/fhir/Basic", &global)).await?;
    assert_eq!(StatusCode::CREATED, status);
    let (status, _) = call(case.app(), with_body("POST", "/fhir/Basic", &preferences())).await?;
    assert_eq!(StatusCode::CREATED, status);
    let code = "https://myapp.example.org%7Cdisplay-preferences";
    let (_, missing) = call(
        case.app(),
        get(&format!("/fhir/Basic?code={code}&subject:missing=true")),
    )
    .await?;
    assert_eq!(Some(1), missing["total"].as_u64(), "{missing}");
    assert!(missing["entry"][0]["resource"].get("subject").is_none());
    let (_, every) = call(case.app(), get(&format!("/fhir/Basic?code={code}"))).await?;
    assert_eq!(Some(2), every["total"].as_u64(), "{every}");
    Ok(())
}

#[tokio::test]
async fn an_update_that_moves_the_subject_or_the_code_is_a_failed_precondition()
-> Result<(), Box<dyn Error>> {
    let Some(case) = Case::new().await? else {
        return Ok(());
    };
    let (_, created) = call(case.app(), with_body("POST", "/fhir/Basic", &keys("a"))).await?;
    let id = created["id"].as_str().ok_or("an id")?.to_owned();
    let path = format!("/fhir/Basic/{id}");
    let mut moved = keys("a");
    moved["id"] = json!(id);
    moved["subject"]["reference"] = json!(at_base("Patient/456"));
    let (status, _) = call(
        case.app(),
        matching(with_body("PUT", &path, &moved), "W/\"1\""),
    )
    .await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, status);
    let mut recoded = keys("a");
    recoded["id"] = json!(id);
    recoded["code"]["coding"][0]["code"] = json!("other");
    let (status, _) = call(
        case.app(),
        matching(with_body("PUT", &path, &recoded), "W/\"1\""),
    )
    .await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, status);
    Ok(())
}

#[tokio::test]
async fn a_write_without_if_match_is_a_failed_precondition() -> Result<(), Box<dyn Error>> {
    let Some(case) = Case::new().await? else {
        return Ok(());
    };
    let (_, created) = call(case.app(), with_body("POST", "/fhir/Basic", &keys("a"))).await?;
    let id = created["id"].as_str().ok_or("an id")?.to_owned();
    let mut update = keys("b");
    update["id"] = json!(id);
    let path = format!("/fhir/Basic/{id}");
    let (status, _) = call(case.app(), with_body("PUT", &path, &update)).await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, status);
    let (status, _) = call(
        case.app(),
        Request::delete(path.as_str()).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, status);
    Ok(())
}

#[tokio::test]
async fn an_id_never_issued_is_not_found() -> Result<(), Box<dyn Error>> {
    let Some(case) = Case::new().await? else {
        return Ok(());
    };
    let mut update = keys("b");
    update["id"] = json!("never");
    let (status, _) = call(
        case.app(),
        matching(with_body("PUT", "/fhir/Basic/never", &update), "W/\"1\""),
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    Ok(())
}

#[tokio::test]
async fn a_search_without_a_system_code_or_with_another_parameter_is_refused()
-> Result<(), Box<dyn Error>> {
    let Some(case) = Case::new().await? else {
        return Ok(());
    };
    for query in [
        "/fhir/Basic",
        "/fhir/Basic?code=display-preferences",
        "/fhir/Basic?code=https://myapp.example.org%7Cx&author=Practitioner/1",
        "/fhir/Basic?code=https://myapp.example.org%7Cx&subject:missing=maybe",
    ] {
        let (status, body) = call(case.app(), get(query)).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {body}");
        assert_eq!(Some("OperationOutcome"), body["resourceType"].as_str());
    }
    Ok(())
}

#[tokio::test]
async fn a_create_breaking_the_profile_is_refused_at_its_element() -> Result<(), Box<dyn Error>> {
    let Some(case) = Case::new().await? else {
        return Ok(());
    };
    let mut with_id = preferences();
    with_id["id"] = json!("app-state");
    let (status, body) = call(case.app(), with_body("POST", "/fhir/Basic", &with_id)).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status);
    assert_eq!(
        Some("Basic.id"),
        body["issue"][0]["location"][0].as_str(),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_declares_the_app_state_endpoint() -> Result<(), Box<dyn Error>> {
    let Some(case) = Case::new().await? else {
        return Ok(());
    };
    let (status, statement) = call(case.app(), get("/fhir/metadata")).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(
        json!(["http://hl7.org/fhir/smart-app-launch/CapabilityStatement/smart-app-state-server"]),
        statement["instantiates"]
    );
    let resources = statement["rest"][0]["resource"]
        .as_array()
        .ok_or("resources")?;
    let basic = resources
        .iter()
        .find(|entry| entry["type"] == json!("Basic"))
        .ok_or("Basic is declared")?;
    assert_eq!(
        json!(["http://hl7.org/fhir/smart-app-launch/StructureDefinition/smart-app-state-basic"]),
        basic["supportedProfile"]
    );
    let interactions: Vec<&str> = basic["interaction"]
        .as_array()
        .ok_or("interactions")?
        .iter()
        .filter_map(|entry| entry["code"].as_str())
        .collect();
    assert_eq!(
        vec!["search-type", "create", "update", "delete"],
        interactions
    );
    Ok(())
}

#[tokio::test]
async fn a_facade_without_app_state_declares_no_basic() -> Result<(), Box<dyn Error>> {
    let harness = super::harness().await;
    let (_, statement) = call(app_over(&harness.facade), get("/fhir/metadata")).await?;
    assert!(statement.get("instantiates").is_none(), "{statement}");
    Ok(())
}
