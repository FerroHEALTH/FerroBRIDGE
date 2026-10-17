// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Checking, saving and reloading mapping files through the operator API,
//! over a facade and an operations lane whose CDR serves the testkit
//! template. No specification governs the operator surface: our own design.

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrobridge_server::check::Report;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use ferrobridge_server::health::Registry;
use ferrobridge_server::operator::Operator;
use ferrobridge_server::operator::files::{Language, Root};
use ferrobridge_server::operator::reload::Reloader;
use ferrobridge_server::operator::snapshot::Snapshot;
use ferrobridge_server::state::{AppState, OperationsLane};
use http::{Request, StatusCode, header};
use jsonwebtoken::Algorithm;
use tower::ServiceExt as _;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

use super::{FIXTURES, OMOCL_FILE, OPERATOR, bytes, digest};
use crate::facade::auth::{Issuer, SigningKey, authority, bearing, ec_key, mint};

mod search;
mod unprovisioned;

/// The analyte file of the OMOCL corpus, provisioned beside the FHIRconnect
/// tree.
const PROVISIONED_OMOCL: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/omocl/medical_data/cluster/Laboratory_test_analyte_v1.yml"
);

/// A model mapping onto `Observation` over the testkit template.
const MODEL: &str = r#"grammar: FHIRConnect/v1.0.0
type: model
metadata:
  name: ferrobridge_console
  version: 1.0.0
spec:
  system: FHIR
  version: R4
  openEhrConfig:
    archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1
  fhirConfig:
    structureDefinition: http://hl7.org/fhir/StructureDefinition/Observation

mappings:
  - name: "problemDiagnose"
    with:
      fhir: "$resource.code"
      openehr: "$archetype/data[at0001]/items[at0002]"
      type: "CODEABLECONCEPT"
"#;

/// The context that puts [`MODEL`] on the wire.
const CONTEXT: &str = r#"grammar: FHIRConnect/v1.0.0
type: context
metadata:
  name: ferrobridge_console.context
  version: 1.0.0
spec:
  system: FHIR
  version: R4
context:
  profile:
    url: "http://example.org/fhir/StructureDefinition/ferrobridge-console-observation"
  template:
    id: "ferrobridge.diagnose.v1"
  archetypes:
    - "ferrobridge_console"
  start: "ferrobridge_console"
"#;

/// A model whose openEHR path names no node of the template.
const BROKEN: &str = r#"grammar: FHIRConnect/v1.0.0
type: model
metadata:
  name: ferrobridge_console
  version: 1.0.0
spec:
  system: FHIR
  version: R4
  openEhrConfig:
    archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1
  fhirConfig:
    structureDefinition: http://hl7.org/fhir/StructureDefinition/Observation

mappings:
  - name: "problemDiagnose"
    with:
      fhir: "$resource.code"
      openehr: "$archetype/data[at0001]/items[at9999]"
      type: "CODEABLECONCEPT"
"#;

/// The reason every save of these cases gives.
const REASON: &str = "synthetic reason for the test";

/// One case: the trees, the CDR, the issuer and the router.
struct Case {
    /// The provisioned tree and the console's, kept alive for the case.
    tree: tempfile::TempDir,
    /// The stub CDR, kept alive for the case.
    cdr: MockServer,
    /// The stub authorization server.
    issuer: Issuer,
    /// The signing key.
    key: SigningKey,
    /// The router under test.
    app: Router,
}

impl Case {
    /// Builds a facade and an operations lane over the provisioned fixtures,
    /// with the operator API saving into an empty console directory.
    async fn new() -> Result<Self, Box<dyn StdError>> {
        let tree = tempfile::tempdir()?;
        let provisioned = tree.path().join("mappings");
        std::fs::create_dir_all(&provisioned)?;
        for name in [
            "ferrobridge_facade.context.yml",
            "ferrobridge_facade.yml",
            "ferrobridge_facade.search.yml",
        ] {
            std::fs::copy(Path::new(FIXTURES).join(name), provisioned.join(name))?;
        }
        let omocl = tree.path().join("omocl");
        std::fs::create_dir_all(&omocl)?;
        std::fs::copy(
            PROVISIONED_OMOCL,
            omocl.join("Laboratory_test_analyte_v1.yml"),
        )?;
        let cdr = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path(
                "/definition/template/adl1.4/ferrobridge.diagnose.v1",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "application/xml")
                    .set_body_string(ferrobridge_testkit::fixtures::DIAGNOSE_OPT),
            )
            .mount(&cdr)
            .await;
        let client = crate::facade::client(&cdr);
        let set = programs::read_set(&provisioned)?;
        let templates = programs::fetch_templates(&set, &client).await?;
        let compiled = programs::compile_set(&set, &templates)?;
        let store = Arc::new(MemoryStore::new());
        let built = Facade::new(
            compiled,
            crate::facade::handle(&store),
            client.clone(),
            crate::facade::settings(),
        );
        let catalog = ferrobridge_server::facade::search::load(&provisioned, &built.programs())?;
        let facade = Arc::new(built.with_search(
            catalog,
            ferrobridge_server::facade::search::Limits::default(),
        ));
        let lane = OperationsLane::new(
            ferrobridge_server::mappings::load_from_cdr(&provisioned, &client).await?,
            "Device/ferrobridge-test",
        );
        let key = ec_key("k1", Algorithm::ES256);
        let issuer = Issuer::start(&[&key]).await;
        let state = AppState::with_health(Registry::default())
            .with_facade(Arc::clone(&facade))
            .serving_operations(lane)
            .with_auth(authority(issuer.settings()).await);
        let reloader = Reloader {
            fhirconnect: Some(provisioned.clone()),
            templates: None,
            cdr: Some(client),
            omocl: true,
            facade: Some(facade),
            operations: state.operations_handle().cloned(),
        };
        let operator = Operator::new(
            vec![
                Root::new(Language::Fhirconnect, provisioned),
                Root::new(Language::Omocl, omocl),
            ],
            Snapshot::new(Report::default(), BTreeMap::new()),
        )
        .with_saving(tree.path().join("console"), reloader);
        let state = state.with_operator(Arc::new(operator));
        let app = ferrobridge_server::router(Arc::new(state), &server_settings());
        Ok(Self {
            tree,
            cdr,
            issuer,
            key,
            app,
        })
    }

    /// Returns the console file `relative` names.
    fn console(&self, relative: &str) -> PathBuf {
        self.tree.path().join("console/fhirconnect").join(relative)
    }

    /// Sends `request` under an operator token.
    async fn send(
        &self,
        request: Request<Body>,
    ) -> Result<http::Response<Body>, Box<dyn StdError>> {
        let token = mint(&self.key, &self.issuer.claims(OPERATOR));
        Ok(self.app.clone().oneshot(bearing(request, &token)).await?)
    }

    /// Saves `body` at the console path `relative` as a new file.
    async fn create(
        &self,
        relative: &str,
        body: &str,
    ) -> Result<http::Response<Body>, Box<dyn StdError>> {
        self.create_at(&format!("fhirconnect/{relative}"), body)
            .await
    }

    /// Saves `body` at the wire path `wire` as a new file.
    async fn create_at(
        &self,
        wire: &str,
        body: &str,
    ) -> Result<http::Response<Body>, Box<dyn StdError>> {
        self.send(
            Request::put(format!("/operator/mappings/{wire}"))
                .header(header::IF_NONE_MATCH, "*")
                .header("ferrobridge-reason", REASON)
                .body(Body::from(body.to_owned()))?,
        )
        .await
    }

    /// Returns the operator state document.
    async fn state(&self) -> Result<serde_json::Value, Box<dyn StdError>> {
        let response = self
            .send(Request::get("/operator/state").body(Body::empty())?)
            .await?;
        assert_eq!(StatusCode::OK, response.status(), "the state route answers");
        Ok(serde_json::from_slice(&bytes(response).await?)?)
    }

    /// Returns the resource types the `CapabilityStatement` names.
    async fn declared_types(&self) -> Result<Vec<String>, Box<dyn StdError>> {
        let response = self
            .app
            .clone()
            .oneshot(Request::get("/fhir/metadata").body(Body::empty())?)
            .await?;
        assert_eq!(
            StatusCode::OK,
            response.status(),
            "the metadata route answers"
        );
        let statement: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
        Ok(statement["rest"][0]["resource"]
            .as_array()
            .ok_or("a resource array")?
            .iter()
            .filter_map(|entry| entry["type"].as_str().map(str::to_owned))
            .collect())
    }

    /// Returns every `..`-prefixed staging entry left under the console tree.
    fn staging_left(&self) -> Vec<PathBuf> {
        let mut left = Vec::new();
        let mut stack = vec![self.tree.path().join("console")];
        while let Some(directory) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if entry.file_name().to_string_lossy().starts_with("..") {
                    let empty_drafts = path.is_dir()
                        && std::fs::read_dir(&path).is_ok_and(|mut inner| inner.next().is_none());
                    if !empty_drafts {
                        left.push(path.clone());
                    }
                }
                if path.is_dir() {
                    stack.push(path);
                }
            }
        }
        left
    }
}

/// Returns server settings whose body ceiling admits a mapping file.
fn server_settings() -> ferrobridge_server::config::ServerSettings {
    ferrobridge_server::config::ServerSettings {
        body_limit: 1024 * 1024,
        ..crate::support::settings()
    }
}

#[tokio::test]
async fn a_saved_file_is_byte_identical_and_the_capability_statement_follows()
-> Result<(), Box<dyn StdError>> {
    let case = Case::new().await?;
    assert!(
        !case
            .declared_types()
            .await?
            .contains(&String::from("Observation"))
    );
    let response = case.create("ferrobridge_console.yml", MODEL).await?;
    assert_eq!(StatusCode::OK, response.status());
    let response = case
        .create("ferrobridge_console.context.yml", CONTEXT)
        .await?;
    assert_eq!(StatusCode::OK, response.status());
    let etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    assert_eq!(Some(format!("\"{}\"", digest(CONTEXT.as_bytes()))), etag);
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(serde_json::Value::Bool(true), report["clean"], "{report}");
    assert_eq!(
        CONTEXT.as_bytes(),
        std::fs::read(case.console("ferrobridge_console.context.yml"))?.as_slice()
    );
    assert_eq!(
        MODEL.as_bytes(),
        std::fs::read(case.console("ferrobridge_console.yml"))?.as_slice()
    );
    let types = case.declared_types().await?;
    assert!(types.contains(&String::from("Observation")), "{types:?}");
    assert!(types.contains(&String::from("Condition")), "{types:?}");
    assert!(case.staging_left().is_empty(), "{:?}", case.staging_left());
    Ok(())
}

#[tokio::test]
async fn the_listing_and_the_file_route_serve_a_saved_file_as_the_consoles()
-> Result<(), Box<dyn StdError>> {
    let case = Case::new().await?;
    assert_eq!(
        StatusCode::OK,
        case.create("ferrobridge_console.yml", MODEL)
            .await?
            .status()
    );
    let response = case
        .send(Request::get("/operator/mappings").body(Body::empty())?)
        .await?;
    let listing: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    let saved = listing
        .as_array()
        .ok_or("a listing")?
        .iter()
        .find(|entry| entry["path"] == "fhirconnect/ferrobridge_console.yml")
        .ok_or("the saved file is listed")?;
    assert_eq!("console", saved["owner"]);
    assert_eq!(serde_json::Value::Bool(false), saved["read_only"]);
    assert_eq!(digest(MODEL.as_bytes()), saved["sha256"]);
    let response = case
        .send(
            Request::get("/operator/mappings/fhirconnect/ferrobridge_console.yml")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(MODEL.as_bytes(), bytes(response).await?.as_slice());
    Ok(())
}

#[tokio::test]
async fn a_failing_draft_is_refused_and_leaves_disk_and_programs_unchanged()
-> Result<(), Box<dyn StdError>> {
    let case = Case::new().await?;
    assert_eq!(
        StatusCode::OK,
        case.create("ferrobridge_console.yml", MODEL)
            .await?
            .status()
    );
    assert_eq!(
        StatusCode::OK,
        case.create("ferrobridge_console.context.yml", CONTEXT)
            .await?
            .status()
    );
    let before = case.declared_types().await?;
    let response = case
        .send(
            Request::put("/operator/mappings/fhirconnect/ferrobridge_console.yml")
                .header(
                    header::IF_MATCH,
                    format!("\"{}\"", digest(MODEL.as_bytes())),
                )
                .header("ferrobridge-reason", REASON)
                .body(Body::from(BROKEN))?,
        )
        .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, response.status());
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(serde_json::Value::Bool(false), report["clean"]);
    let diagnostic = report["diagnostics"]
        .as_array()
        .ok_or("a diagnostics array")?
        .iter()
        .find(|diagnostic| diagnostic["code"] == "fc-unknown-template-node")
        .ok_or("the unknown node is reported")?;
    assert_eq!("fhirconnect/ferrobridge_console.yml", diagnostic["file"]);
    assert_eq!(
        MODEL.as_bytes(),
        std::fs::read(case.console("ferrobridge_console.yml"))?.as_slice(),
        "the saved file keeps its bytes"
    );
    assert!(case.staging_left().is_empty(), "{:?}", case.staging_left());
    assert_eq!(before, case.declared_types().await?);
    assert!(before.contains(&String::from("Observation")), "{before:?}");
    Ok(())
}

#[tokio::test]
async fn a_stale_if_match_answers_412_and_the_current_one_saves() -> Result<(), Box<dyn StdError>> {
    let case = Case::new().await?;
    assert_eq!(
        StatusCode::OK,
        case.create("ferrobridge_console.yml", MODEL)
            .await?
            .status()
    );
    let revised = MODEL.replace("version: 1.0.0", "version: 1.0.1");
    let update = |tag: String| {
        Request::put("/operator/mappings/fhirconnect/ferrobridge_console.yml")
            .header(header::IF_MATCH, tag)
            .header("ferrobridge-reason", REASON)
            .body(Body::from(revised.clone()))
    };
    let stale = case.send(update(String::from("\"0000\""))?).await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, stale.status());
    assert_eq!(
        MODEL.as_bytes(),
        std::fs::read(case.console("ferrobridge_console.yml"))?.as_slice()
    );
    let current = case
        .send(update(format!("\"{}\"", digest(MODEL.as_bytes())))?)
        .await?;
    assert_eq!(StatusCode::OK, current.status());
    assert_eq!(
        revised.as_bytes(),
        std::fs::read(case.console("ferrobridge_console.yml"))?.as_slice()
    );
    let again = case.create("ferrobridge_console.yml", MODEL).await?;
    assert_eq!(
        StatusCode::PRECONDITION_FAILED,
        again.status(),
        "If-None-Match: * refuses an existing file"
    );
    Ok(())
}

#[tokio::test]
async fn a_save_without_a_precondition_or_a_reason_is_refused() -> Result<(), Box<dyn StdError>> {
    let case = Case::new().await?;
    let unconditional = case
        .send(
            Request::put("/operator/mappings/fhirconnect/ferrobridge_console.yml")
                .header("ferrobridge-reason", REASON)
                .body(Body::from(MODEL))?,
        )
        .await?;
    assert_eq!(StatusCode::PRECONDITION_REQUIRED, unconditional.status());
    let reasonless = case
        .send(
            Request::put("/operator/mappings/fhirconnect/ferrobridge_console.yml")
                .header(header::IF_NONE_MATCH, "*")
                .body(Body::from(MODEL))?,
        )
        .await?;
    assert_eq!(StatusCode::BAD_REQUEST, reasonless.status());
    assert!(!case.console("ferrobridge_console.yml").exists());
    Ok(())
}

#[tokio::test]
async fn a_save_over_a_provisioned_path_is_refused() -> Result<(), Box<dyn StdError>> {
    let case = Case::new().await?;
    let response = case
        .send(
            Request::put("/operator/mappings/fhirconnect/ferrobridge_facade.yml")
                .header(header::IF_MATCH, "*")
                .header("ferrobridge-reason", REASON)
                .body(Body::from(MODEL))?,
        )
        .await?;
    assert_eq!(StatusCode::CONFLICT, response.status());
    assert!(!case.console("ferrobridge_facade.yml").exists());
    for path in [
        "/operator/mappings/fhirconnect/../escape.yml",
        "/operator/mappings/fhirconnect/notes.txt",
    ] {
        let response = case
            .send(
                Request::put(path)
                    .header(header::IF_NONE_MATCH, "*")
                    .header("ferrobridge-reason", REASON)
                    .body(Body::from(MODEL))?,
            )
            .await?;
        assert_eq!(StatusCode::BAD_REQUEST, response.status(), "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn a_draft_check_answers_its_diagnostics_and_writes_nothing() -> Result<(), Box<dyn StdError>>
{
    let case = Case::new().await?;
    let check = |body: &str| {
        Request::post("/operator/mappings/check?path=fhirconnect/ferrobridge_facade.yml")
            .body(Body::from(body.to_owned()))
    };
    let fixture = std::fs::read_to_string(Path::new(FIXTURES).join("ferrobridge_facade.yml"))?;
    let broken = fixture.replace("items[at0002]", "items[at9999]");
    let response = case.send(check(&broken)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(serde_json::Value::Bool(false), report["clean"]);
    assert!(
        report["diagnostics"]
            .as_array()
            .ok_or("a diagnostics array")?
            .iter()
            .any(
                |diagnostic| diagnostic["file"] == "fhirconnect/ferrobridge_facade.yml"
                    && diagnostic["code"] == "fc-unknown-template-node"
            ),
        "{report}"
    );
    let response = case.send(check(&fixture)?).await?;
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(serde_json::Value::Bool(true), report["clean"], "{report}");
    assert!(!case.console("ferrobridge_console.yml").exists());
    assert!(case.staging_left().is_empty(), "{:?}", case.staging_left());
    Ok(())
}

#[tokio::test]
async fn a_reload_picks_up_a_file_placed_in_the_console_directory() -> Result<(), Box<dyn StdError>>
{
    let case = Case::new().await?;
    std::fs::create_dir_all(case.console(""))?;
    std::fs::write(case.console("ferrobridge_console.yml"), MODEL)?;
    std::fs::write(case.console("ferrobridge_console.context.yml"), CONTEXT)?;
    let response = case
        .send(Request::post("/operator/reload").body(Body::empty())?)
        .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        case.declared_types()
            .await?
            .contains(&String::from("Observation"))
    );
    Ok(())
}

#[test]
fn the_save_audit_line_names_the_caller_file_digests_and_reason_and_no_content()
-> Result<(), Box<dyn StdError>> {
    let logs = crate::support::Logs::default();
    let capture = ferrobridge_server::telemetry::subscriber(
        ferrobridge_server::telemetry::Rendering::Json,
        "info",
        false,
        logs.clone(),
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    tracing::subscriber::with_default(capture, || {
        runtime.block_on(async {
            let case = Case::new().await?;
            let response = case.create("ferrobridge_console.yml", MODEL).await?;
            assert_eq!(StatusCode::OK, response.status());
            Ok::<(), Box<dyn StdError>>(())
        })
    })?;
    let text = logs.text();
    let saves: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|line| line["target"] == "ferrobridge::audit" && line["action"] == "save")
        .collect();
    assert_eq!(1, saves.len(), "{text}");
    let save = &saves[0];
    assert_eq!("synthetic-client-sub", save["subject"]);
    assert_eq!("fhirconnect/ferrobridge_console.yml", save["file"]);
    assert_eq!("-", save["old_sha256"]);
    assert_eq!(digest(MODEL.as_bytes()), save["new_sha256"]);
    assert_eq!(REASON, save["reason"]);
    assert_eq!(200, save["status"]);
    for marker in [
        "problemDiagnose",
        "items[at0002]",
        "structureDefinition",
        "Observation",
    ] {
        assert!(
            !text.contains(marker),
            "the log carries mapping content `{marker}`: {text}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_omocl_file_saves_under_the_console_tree_and_joins_the_set()
-> Result<(), Box<dyn StdError>> {
    let case = Case::new().await?;
    let specimen = std::fs::read_to_string(OMOCL_FILE)?;
    let response = case.create_at("omocl/Specimen_v1.yml", &specimen).await?;
    assert_eq!(StatusCode::OK, response.status());
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!(serde_json::Value::Bool(true), report["clean"], "{report}");
    assert_eq!(
        specimen.as_bytes(),
        std::fs::read(case.tree.path().join("console/omocl/Specimen_v1.yml"))?.as_slice()
    );
    let state = case.state().await?;
    assert_eq!(
        serde_json::json!([
            "omocl/Laboratory_test_analyte_v1.yml",
            "omocl/Specimen_v1.yml"
        ]),
        state["omocl"]["files"]
    );
    let response = case
        .send(Request::get("/operator/mappings").body(Body::empty())?)
        .await?;
    let listing: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    let saved = listing
        .as_array()
        .ok_or("a listing")?
        .iter()
        .find(|entry| entry["path"] == "omocl/Specimen_v1.yml")
        .ok_or("the saved file is listed")?;
    assert_eq!("console", saved["owner"]);
    assert_eq!("mapping", saved["kind"]);
    assert!(case.staging_left().is_empty(), "{:?}", case.staging_left());
    Ok(())
}

#[tokio::test]
async fn an_omocl_draft_that_does_not_load_is_refused_and_writes_nothing()
-> Result<(), Box<dyn StdError>> {
    let case = Case::new().await?;
    let response = case
        .create_at("omocl/broken.yml", "grammar: [unclosed\n")
        .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, response.status());
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert!(
        report["diagnostics"]
            .as_array()
            .ok_or("a diagnostics array")?
            .iter()
            .any(|diagnostic| diagnostic["file"] == "omocl/broken.yml"),
        "{report}"
    );
    assert!(!case.tree.path().join("console/omocl/broken.yml").exists());
    assert!(case.staging_left().is_empty(), "{:?}", case.staging_left());
    Ok(())
}

#[tokio::test]
async fn an_omocl_clash_with_a_provisioned_file_is_refused() -> Result<(), Box<dyn StdError>> {
    let case = Case::new().await?;
    let analyte = std::fs::read_to_string(PROVISIONED_OMOCL)?;
    let response = case
        .send(
            Request::put("/operator/mappings/omocl/Laboratory_test_analyte_v1.yml")
                .header(header::IF_MATCH, "*")
                .header("ferrobridge-reason", REASON)
                .body(Body::from(analyte.clone()))?,
        )
        .await?;
    assert_eq!(
        StatusCode::CONFLICT,
        response.status(),
        "a provisioned path is never written"
    );
    let response = case.create_at("omocl/analyte_copy.yml", &analyte).await?;
    assert_eq!(
        StatusCode::UNPROCESSABLE_ENTITY,
        response.status(),
        "a second file defining a provisioned mapping is refused"
    );
    let console = case.tree.path().join("console/omocl");
    assert!(!console.join("Laboratory_test_analyte_v1.yml").exists());
    assert!(!console.join("analyte_copy.yml").exists());
    Ok(())
}
