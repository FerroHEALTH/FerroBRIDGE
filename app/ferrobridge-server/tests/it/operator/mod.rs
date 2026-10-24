// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The read-only operator API behind `[operator] enabled` and the operator
//! scope, over a `wiremock` authorization server and a synthetic mapping
//! tree. No specification governs the operator surface: our own design.

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrobridge_server::check::{self, Inputs, Templates};
use ferrobridge_server::health::Registry;
use ferrobridge_server::operator::Operator;
use ferrobridge_server::operator::files::{Language, Root};
use ferrobridge_server::operator::snapshot::Snapshot;
use ferrobridge_server::smart::Smart;
use ferrobridge_server::state::AppState;
use http::{Request, StatusCode, header};
use jsonwebtoken::Algorithm;
use tower::ServiceExt as _;

use crate::facade::auth::{Issuer, SigningKey, authority, bearing, challenge, ec_key, mint};

mod configuration;
mod save;

/// The clean mapping set the facade suite runs.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures");

/// The laboratory result file of the OMOCL corpus.
const OMOCL_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/omocl/medical_data/cluster/Specimen_v1.yml"
);

/// The operator scope every admitted token carries.
const OPERATOR: &str = "__ferrobridge.operator";

/// One case: a mapping tree, an issuer, its key and the router.
struct Case {
    /// The mapping tree, kept alive for the case.
    tree: tempfile::TempDir,
    /// The stub authorization server.
    issuer: Issuer,
    /// The signing key.
    key: SigningKey,
    /// The router under test.
    app: Router,
}

impl Case {
    /// Builds the tree, the issuer and the router.
    async fn new() -> Result<Self, Box<dyn Error>> {
        Self::answering(None).await
    }

    /// Builds the tree, the issuer and the router, the operator API
    /// answering `configuration` as the running configuration when given.
    async fn answering(
        configuration: Option<ferrobridge_operator_api::configuration::Configuration>,
    ) -> Result<Self, Box<dyn Error>> {
        let tree = tempfile::tempdir()?;
        let mappings = tree.path().join("mappings");
        let templates = tree.path().join("templates");
        let omocl = tree.path().join("omocl");
        for directory in [&mappings, &templates, &omocl] {
            std::fs::create_dir_all(directory)?;
        }
        for name in [
            "ferrobridge_facade.context.yml",
            "ferrobridge_facade.yml",
            "ferrobridge_facade.search.yml",
        ] {
            std::fs::copy(Path::new(FIXTURES).join(name), mappings.join(name))?;
        }
        std::fs::write(
            templates.join("diagnose.opt"),
            ferrobridge_testkit::fixtures::DIAGNOSE_OPT,
        )?;
        std::fs::copy(OMOCL_FILE, omocl.join("Specimen_v1.yml"))?;
        let key = ec_key("k1", Algorithm::ES256);
        let issuer = Issuer::start(&[&key]).await;
        let authority: Arc<Smart> = authority(issuer.settings()).await;
        let report = check::check(Inputs {
            fhirconnect: Some(&mappings),
            templates: Some(Templates::Directory(&templates)),
            omocl: Some(&omocl),
        })
        .await?;
        let roots = vec![
            Root::new(Language::Fhirconnect, mappings),
            Root::new(Language::Omocl, omocl),
        ];
        let operator = Operator::new(
            roots,
            Snapshot::new(report, std::collections::BTreeMap::new()),
        );
        let operator = match configuration {
            Some(configuration) => operator.with_configuration(configuration),
            None => operator,
        };
        let state = AppState::with_health(Registry::default())
            .with_auth(authority)
            .with_operator(Arc::new(operator));
        let app = ferrobridge_server::router(Arc::new(state), &crate::support::settings());
        Ok(Self {
            tree,
            issuer,
            key,
            app,
        })
    }

    /// Returns a token granting `scope`.
    fn token(&self, scope: &str) -> String {
        mint(&self.key, &self.issuer.claims(scope))
    }

    /// Sends a `GET` of `path`, under a token granting `scope` when given.
    async fn get(
        &self,
        path: &str,
        scope: Option<&str>,
    ) -> Result<http::Response<Body>, Box<dyn Error>> {
        let request = Request::get(path).body(Body::empty())?;
        let request = match scope {
            Some(scope) => bearing(request, &self.token(scope)),
            None => request,
        };
        Ok(self.app.clone().oneshot(request).await?)
    }
}

/// Reads the body of `response` as bytes.
async fn bytes(response: http::Response<Body>) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await?
        .to_vec())
}

/// Returns the lowercase hexadecimal SHA-256 of `bytes`.
fn digest(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            use std::fmt::Write as _;
            let _written = write!(hex, "{byte:02x}");
            hex
        })
}

/// The read-only operator routes, as a client calls them.
const ROUTES: [&str; 4] = [
    "/operator/state",
    "/operator/configuration",
    "/operator/mappings",
    "/operator/mappings/fhirconnect/ferrobridge_facade.yml",
];

#[tokio::test]
async fn every_route_answers_401_without_a_token() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    for route in ROUTES {
        let response = case.get(route, None).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, response.status(), "{route}");
        assert_eq!(
            Some(String::from("Bearer")),
            challenge(&response),
            "{route}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn every_route_answers_403_without_the_operator_scope() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    for route in ROUTES {
        let response = case.get(route, Some("system/*.cruds openid")).await?;
        assert_eq!(StatusCode::FORBIDDEN, response.status(), "{route}");
        assert_eq!(
            Some(String::from("Bearer error=\"insufficient_scope\"")),
            challenge(&response),
            "{route}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_state_names_each_compiled_context_and_where_it_came_from() -> Result<(), Box<dyn Error>>
{
    let case = Case::new().await?;
    let response = case.get("/operator/state", Some(OPERATOR)).await?;
    assert_eq!(StatusCode::OK, response.status());
    let state: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    let context = &state["contexts"][0];
    assert_eq!("ferrobridge_facade.context", context["context"]);
    assert_eq!(
        "fhirconnect/ferrobridge_facade.context.yml",
        context["file"]
    );
    assert_eq!("ferrobridge.diagnose.v1", context["template"]);
    assert_eq!("Condition", context["resource_type"]);
    assert_eq!(
        "http://example.org/fhir/StructureDefinition/ferrobridge-facade-diagnosis",
        context["profile"]
    );
    assert_eq!("ferrobridge_facade", context["models"][0]["name"]);
    assert_eq!(
        "fhirconnect/ferrobridge_facade.yml",
        context["models"][0]["file"]
    );
    assert_eq!(serde_json::json!([]), context["models"][0]["extensions"]);
    assert_eq!("directory", state["templates"][0]["source"]);
    assert_eq!("diagnose.opt", state["templates"][0]["file"]);
    assert_eq!(
        serde_json::json!(["omocl/Specimen_v1.yml"]),
        state["omocl"]["files"]
    );
    assert_eq!(serde_json::Value::Bool(true), state["omocl"]["valid"]);
    let text = state.to_string();
    let root = case.tree.path().to_string_lossy().into_owned();
    assert!(
        !text.contains(&root),
        "the document names no host directory: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn the_listing_names_every_file_with_its_digest_and_owner() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let response = case.get("/operator/mappings", Some(OPERATOR)).await?;
    assert_eq!(StatusCode::OK, response.status());
    let listing: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    let entries = listing.as_array().ok_or("a listing array")?;
    let kinds: Vec<(&str, &str)> = entries
        .iter()
        .map(|entry| {
            (
                entry["path"].as_str().unwrap_or_default(),
                entry["kind"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        vec![
            ("fhirconnect/ferrobridge_facade.context.yml", "context"),
            ("fhirconnect/ferrobridge_facade.search.yml", "search"),
            ("fhirconnect/ferrobridge_facade.yml", "model"),
            ("omocl/Specimen_v1.yml", "mapping"),
        ],
        kinds
    );
    for entry in entries {
        let path = entry["path"].as_str().ok_or("a path")?;
        let (root, relative) = path.split_once('/').ok_or("a rooted path")?;
        let directory = if root == "fhirconnect" {
            "mappings"
        } else {
            "omocl"
        };
        let on_disk = std::fs::read(case.tree.path().join(directory).join(relative))?;
        assert_eq!(digest(&on_disk), entry["sha256"], "{path}");
        assert_eq!("provisioned", entry["owner"], "{path}");
        assert_eq!(serde_json::Value::Bool(true), entry["read_only"], "{path}");
        assert_eq!(serde_json::json!([]), entry["diagnostics"], "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn a_file_is_served_verbatim_with_its_digest_as_the_etag() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    for (route, on_disk) in [
        (
            "/operator/mappings/fhirconnect/ferrobridge_facade.yml",
            case.tree.path().join("mappings/ferrobridge_facade.yml"),
        ),
        (
            "/operator/mappings/omocl/Specimen_v1.yml",
            case.tree.path().join("omocl/Specimen_v1.yml"),
        ),
    ] {
        let response = case.get(route, Some(OPERATOR)).await?;
        assert_eq!(StatusCode::OK, response.status(), "{route}");
        let expected = std::fs::read(&on_disk)?;
        let etag = response
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        assert_eq!(Some(format!("\"{}\"", digest(&expected))), etag, "{route}");
        assert_eq!(expected, bytes(response).await?, "{route}");
    }
    Ok(())
}

#[tokio::test]
async fn a_path_that_names_no_mapping_file_answers_404() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    for route in [
        "/operator/mappings/fhirconnect/absent.yml",
        "/operator/mappings/templates/diagnose.opt",
        "/operator/mappings/fhirconnect",
    ] {
        let response = case.get(route, Some(OPERATOR)).await?;
        assert_eq!(StatusCode::NOT_FOUND, response.status(), "{route}");
    }
    Ok(())
}

#[tokio::test]
async fn a_path_that_escapes_its_directory_answers_404() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let outside = case.tree.path().join("outside.yml");
    std::fs::write(&outside, "secret: outside the mapping tree\n")?;
    std::os::unix::fs::symlink(&outside, case.tree.path().join("mappings/link.yml"))?;
    for route in [
        "/operator/mappings/fhirconnect/../outside.yml",
        "/operator/mappings/fhirconnect/%2e%2e/outside.yml",
        "/operator/mappings/fhirconnect/link.yml",
        "/operator/mappings/fhirconnect/%2Fetc%2Fhosts",
    ] {
        let response = case.get(route, Some(OPERATOR)).await?;
        assert_eq!(StatusCode::NOT_FOUND, response.status(), "{route}");
        let body = String::from_utf8(bytes(response).await?)?;
        assert!(!body.contains("secret"), "{route}: {body}");
    }
    Ok(())
}

#[test]
fn the_audit_line_names_the_caller_and_the_route_and_never_the_content()
-> Result<(), Box<dyn Error>> {
    let logs = crate::support::Logs::default();
    let capture = ferrobridge_server::telemetry::subscriber(
        ferrobridge_server::telemetry::Rendering::Json,
        "info",
        false,
        logs.clone(),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let served = tracing::subscriber::with_default(capture, || {
        runtime.block_on(async {
            let case = Case::new().await?;
            let response = case
                .get(
                    "/operator/mappings/fhirconnect/ferrobridge_facade.yml",
                    Some(OPERATOR),
                )
                .await?;
            assert_eq!(StatusCode::OK, response.status());
            bytes(response).await
        })
    })?;
    let text = logs.text();
    let audit: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|line| line["target"] == ferrohealth_smart::audit::AUDIT_TARGET)
        .collect();
    assert_eq!(1, audit.len(), "{text}");
    assert_eq!("synthetic-client-sub", audit[0]["principal"], "{text}");
    assert_eq!("operator", audit[0]["interaction"], "{text}");
    assert_eq!("/operator/mappings/{*path}", audit[0]["route"], "{text}");
    assert_eq!(200, audit[0]["status"], "{text}");
    let content = String::from_utf8(served)?;
    for line in content.lines().filter(|line| line.contains("openehr:")) {
        assert!(
            !text.contains(line.trim()),
            "the log carries mapping content: {text}"
        );
    }
    assert!(!text.contains("ferrobridge_facade.yml"), "{text}");
    Ok(())
}

#[tokio::test]
async fn without_a_mapping_dir_the_api_checks_and_saves_nothing() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let token = case.token(OPERATOR);
    for request in [
        Request::put("/operator/mappings/fhirconnect/new.yml")
            .header(header::IF_NONE_MATCH, "*")
            .header("ferrobridge-reason", "synthetic")
            .body(Body::from("grammar: FHIRConnect/v1.0.0\n"))?,
        Request::post("/operator/mappings/check?path=fhirconnect/new.yml")
            .body(Body::from("grammar: FHIRConnect/v1.0.0\n"))?,
    ] {
        let response = case.app.clone().oneshot(bearing(request, &token)).await?;
        assert_eq!(StatusCode::CONFLICT, response.status());
    }
    assert!(!case.tree.path().join("mappings/new.yml").exists());
    Ok(())
}
