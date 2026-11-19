// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART launch `Task`s on the facade (SMART App Launch 2.2.0, Task profile
//! for requesting a SMART launch): the guide's two example `Task`s
//! round-tripped, the search by `patient`, `status` and `code`, the
//! refusal of a `Task` of neither profile, and the `CapabilityStatement`.

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

use super::Harness;
use super::app_over;
use super::call;
use super::client;
use super::handle;
use super::settings;

/// The base the guide's examples name their patient under.
pub(super) const EXAMPLE_BASE: &str = "https://example.org/fhir";

/// `Task-task-for-ehr-launch.json` of the `hl7.fhir.uv.smart-app-launch`
/// 2.2.0 package, verbatim.
pub(super) const EHR_LAUNCH: &str = r#"{"resourceType":"Task","id":"task-for-ehr-launch","meta":{"profile":["http://hl7.org/fhir/smart-app-launch/StructureDefinition/task-ehr-launch"]},"status":"requested","intent":"proposal","code":{"coding":[{"system":"http://hl7.org/fhir/smart-app-launch/CodeSystem/smart-codes","code":"launch-app-ehr","display":"Launch application using the SMART EHR launch"}]},"for":{"reference":"https://example.org/fhir/Patient/123"},"encounter":{"reference":"https://example.org/fhir/Encounter/456"},"input":[{"type":{"coding":[{"system":"http://hl7.org/fhir/smart-app-launch/CodeSystem/smart-codes","code":"smartonfhir-application","display":"SMART on FHIR application URL."}]},"valueUrl":"https://www.example.org/myapplication"},{"type":{"coding":[{"system":"http://hl7.org/fhir/smart-app-launch/CodeSystem/smart-codes","code":"smartonfhir-appcontext","display":"Application context related to this launch."}]},"valueString":"{\"field1\":\"value\"}"}]}"#;

/// `Task-task-for-standalone-launch.json` of the same package, verbatim.
pub(super) const STANDALONE_LAUNCH: &str = r#"{"resourceType":"Task","id":"task-for-standalone-launch","meta":{"profile":["http://hl7.org/fhir/smart-app-launch/StructureDefinition/task-standalone-launch"]},"status":"requested","intent":"proposal","code":{"coding":[{"system":"http://hl7.org/fhir/smart-app-launch/CodeSystem/smart-codes","code":"launch-app-standalone","display":"Launch application using the SMART standalone launch"}]},"for":{"reference":"https://example.org/fhir/Patient/123"},"encounter":{"reference":"https://example.org/fhir/Encounter/456"},"input":[{"type":{"coding":[{"system":"http://hl7.org/fhir/smart-app-launch/CodeSystem/smart-codes","code":"smartonfhir-application","display":"SMART on FHIR application URL."}]},"valueUrl":"https://www.example.org/myapplication"}]}"#;

/// One case: the store's directory and the harness over it.
pub(super) struct Case {
    /// The directory the store lives in, removed on drop.
    _directory: tempfile::TempDir,
    /// The harness, its facade serving launch `Task`s under [`EXAMPLE_BASE`].
    pub(super) harness: Harness,
}

impl Case {
    /// Returns a facade with no program, serving launch `Task`s.
    pub(super) async fn new() -> Result<Self, Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let owned = OwnedStore::open(&directory.path().join("launch-tasks.redb"))?;
        let cdr = MockServer::start().await;
        let store = Arc::new(MemoryStore::default());
        let mut facade_settings = settings();
        facade_settings.base_url = String::from(EXAMPLE_BASE);
        let facade = Facade::new(
            Programs::default(),
            handle(&store),
            client(&cdr),
            facade_settings,
        )
        .with_launch_tasks(Some(Arc::new(owned)));
        Ok(Self {
            _directory: directory,
            harness: Harness {
                cdr,
                store,
                facade: Arc::new(facade),
            },
        })
    }

    /// Returns the open router over the facade.
    fn app(&self) -> Router {
        app_over(&self.harness.facade)
    }
}

/// Returns a `POST [base]/Task` carrying `body`.
pub(super) fn post_task(body: &str) -> Request<Body> {
    Request::post("/fhir/Task")
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(body.to_owned()))
        .expect("a legal request")
}

/// Returns a `GET` of `path`.
pub(super) fn get(path: &str) -> Request<Body> {
    Request::get(path)
        .body(Body::empty())
        .expect("a legal request")
}

/// Returns `resource` without the elements the server writes, `id` and
/// `meta.versionId` and `meta.lastUpdated`.
fn content(mut resource: Value) -> Value {
    if let Some(object) = resource.as_object_mut() {
        object.remove("id");
        if let Some(meta) = object.get_mut("meta").and_then(Value::as_object_mut) {
            meta.remove("versionId");
            meta.remove("lastUpdated");
        }
    }
    resource
}

#[tokio::test]
async fn the_guide_examples_round_trip() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    for example in [EHR_LAUNCH, STANDALONE_LAUNCH] {
        let sent: Value = serde_json::from_str(example)?;
        let (status, created) = call(case.app(), post_task(example)).await?;
        assert_eq!(StatusCode::CREATED, status, "{created}");
        let id = created["id"].as_str().ok_or("an id")?.to_owned();
        // NOTE: FHIR R4 http.html#create, "If an id is provided, the server SHALL ignore it".
        assert_ne!(sent["id"], created["id"]);
        assert_eq!(Some("1"), created["meta"]["versionId"].as_str());
        let (status, read) = call(case.app(), get(&format!("/fhir/Task/{id}"))).await?;
        assert_eq!(StatusCode::OK, status, "{read}");
        assert_eq!(created, read);
        assert_eq!(content(sent), content(read));
    }
    Ok(())
}

#[tokio::test]
async fn a_search_finds_the_tasks_by_patient_status_and_code() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    for example in [EHR_LAUNCH, STANDALONE_LAUNCH] {
        let (status, _) = call(case.app(), post_task(example)).await?;
        assert_eq!(StatusCode::CREATED, status);
    }
    for (query, total) in [
        ("patient=Patient/123", 2),
        ("patient=123", 2),
        ("patient=https://example.org/fhir/Patient/123", 2),
        ("patient=Patient/456", 0),
        ("status=requested", 2),
        ("status=http://hl7.org/fhir/task-status%7Crequested", 2),
        ("status=completed", 0),
        (
            "code=http://hl7.org/fhir/smart-app-launch/CodeSystem/smart-codes%7Claunch-app-ehr",
            1,
        ),
        ("code=launch-app-standalone", 1),
        ("code=launch-app-ehr,launch-app-standalone", 2),
        (
            "patient=Patient/123&code=launch-app-ehr&status=requested",
            1,
        ),
    ] {
        let (status, bundle) = call(case.app(), get(&format!("/fhir/Task?{query}"))).await?;
        assert_eq!(StatusCode::OK, status, "{query}: {bundle}");
        assert_eq!(Some("searchset"), bundle["type"].as_str());
        assert_eq!(Some(total), bundle["total"].as_u64(), "{query}: {bundle}");
    }
    let (status, _) = call(case.app(), get("/fhir/Task?owner=Practitioner/1")).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status);
    Ok(())
}

#[tokio::test]
async fn a_task_of_neither_launch_profile_is_refused() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let mut task: Value = serde_json::from_str(STANDALONE_LAUNCH)?;
    task.as_object_mut().ok_or("an object")?.remove("input");
    let (status, body) = call(case.app(), post_task(&task.to_string())).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    assert_eq!(Some("Task.input"), body["issue"][0]["location"][0].as_str());
    let mut fulfil: Value = serde_json::from_str(EHR_LAUNCH)?;
    fulfil["code"] = json!({"coding": [{"system": "http://hl7.org/fhir/CodeSystem/task-code", "code": "fulfill"}]});
    let (status, body) = call(case.app(), post_task(&fulfil.to_string())).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    assert_eq!(Some("Task.code"), body["issue"][0]["location"][0].as_str());
    Ok(())
}

#[tokio::test]
async fn an_id_never_issued_is_not_found() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let (status, _) = call(case.app(), get("/fhir/Task/never")).await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_declares_both_launch_profiles() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let (_, statement) = call(case.app(), get("/fhir/metadata")).await?;
    let task = statement["rest"][0]["resource"]
        .as_array()
        .ok_or("resources")?
        .iter()
        .find(|entry| entry["type"] == json!("Task"))
        .ok_or("Task is declared")?;
    assert_eq!(
        json!([
            "http://hl7.org/fhir/smart-app-launch/StructureDefinition/task-ehr-launch",
            "http://hl7.org/fhir/smart-app-launch/StructureDefinition/task-standalone-launch"
        ]),
        task["supportedProfile"]
    );
    let parameters: Vec<&str> = task["searchParam"]
        .as_array()
        .ok_or("parameters")?
        .iter()
        .filter_map(|entry| entry["name"].as_str())
        .collect();
    assert_eq!(vec!["patient", "status", "code"], parameters);
    Ok(())
}
