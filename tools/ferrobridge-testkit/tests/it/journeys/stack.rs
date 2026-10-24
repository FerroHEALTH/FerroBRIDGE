// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stack a journey runs against, each part on loopback: a `wiremock`
//! CDR, the test OpenID Provider, a running bridge over a synthetic mapping
//! tree with its FHIR facade, operations lane and operator API, and a
//! running console that serves its release site bundle and calls that
//! bridge.
//!
//! The bridge runs `ferrobridge serve`'s own run path,
//! `ferrobridge_server::boot::serve_settings`. Its mapping tree holds one
//! provisioned context and model over the testkit's diagnosis template, and
//! an empty directory the console saves into.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ferrobridge_server::boot::{Backoff, serve_settings};
use ferrobridge_testkit::oidc::Provider;
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The environment variable naming the site bundle's directory, when it is
/// not the workspace's `target/site`.
const SITE_ENV: &str = "FERROBRIDGE_JOURNEYS_SITE";

/// The console's client id at the provider.
const CLIENT_ID: &str = "ferrobridge-console";

/// The scope of the operator's access token: the operator API, and every
/// FHIR interaction the playground and the capability read call.
const SCOPE: &str = "openid __ferrobridge.operator system/*.cruds";

/// The profile the provisioned context claims.
pub(crate) const PROFILE: &str =
    "http://example.org/fhir/StructureDefinition/ferrobridge-journey-diagnose";

/// The name of the provisioned context.
pub(crate) const CONTEXT: &str = "ferrobridge_journey_diagnose.context";

/// The wire path of the provisioned model mapping.
pub(crate) const MODEL_PATH: &str = "fhirconnect/ferrobridge_journey_diagnose.yml";

/// The provisioned context mapping over the testkit's diagnosis template.
pub(crate) const CONTEXT_FILE: &str = r#"grammar: FHIRConnect/v1.0.0
type: context
metadata:
  name: ferrobridge_journey_diagnose.context
  version: 1.0.0
spec:
  system: FHIR
  version: R4
context:
  profile:
    url: "http://example.org/fhir/StructureDefinition/ferrobridge-journey-diagnose"
  template:
    id: "ferrobridge.diagnose.v1"
  archetypes:
    - "ferrobridge_journey_diagnose"
  start: "ferrobridge_journey_diagnose"
"#;

/// The provisioned model mapping: the problem name and the subject.
const MODEL_FILE: &str = r#"grammar: FHIRConnect/v1.0.0
type: model
metadata:
  name: ferrobridge_journey_diagnose
  version: 1.0.0
spec:
  system: FHIR
  version: R4
  openEhrConfig:
    archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1
  fhirConfig:
    structureDefinition: http://hl7.org/fhir/StructureDefinition/Condition

mappings:
  - name: "problemDiagnose"
    with:
      fhir: "$resource.code"
      openehr: "$archetype/data[at0001]/items[at0002]"
      type: "CODEABLECONCEPT"

  - name: "subject"
    unidirectional: "openehr->fhir"
    with:
      fhir: "$resource.subject"
    manual:
      - name: "subjectReference"
        fhir:
          - path: "reference"
            value: "Patient/synthetic-subject-0001"
"#;

/// Which lanes the bridge runs.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Lanes {
    /// Every lane: the facade, the operations and the operator API.
    All,
    /// Every lane but the operations, which `[operations] enabled = false`
    /// turns off.
    WithoutOperations,
}

/// The running stack.
#[derive(Debug)]
pub(crate) struct Stack {
    /// The CDR, kept so it outlives the bridge.
    _cdr: MockServer,
    /// The mapping tree, the templates and the identity store.
    tree: tempfile::TempDir,
    /// The test OpenID Provider.
    provider: Provider,
    /// The console's base URL.
    console: String,
    /// The name of the console's session cookie.
    cookie: String,
    /// The operator's access token, which never reaches the browser.
    access_token: String,
}

impl Stack {
    /// Starts the CDR, the provider, the bridge with every lane, and the
    /// console.
    pub(crate) async fn start() -> Result<Self, Box<dyn Error>> {
        Self::start_with(Lanes::All).await
    }

    /// Starts the stack with the bridge running `lanes`.
    pub(crate) async fn start_with(lanes: Lanes) -> Result<Self, Box<dyn Error>> {
        let site = site_root()?;
        let cdr = cdr().await;
        let tree = tree()?;
        let provider = Provider::start().await?;
        provider.publish_metadata().await;
        let bridge = bridge(tree.path(), &cdr, &provider, lanes).await?;

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let console = format!("http://{}", listener.local_addr()?);
        let access_token = provider.access_token(&format!("{bridge}/fhir"), SCOPE)?;
        provider.approve(CLIENT_ID, &access_token).await;
        let text = format!(
            "[server]\nsite_root = {site}\n\n[bridge]\nbase_url = \"{bridge}/\"\n\n\
             [session]\nsecure_cookie = false\n\n\
             [oidc]\nissuer = \"{issuer}\"\nauthorization_endpoint = \"{authorization}\"\n\
             token_endpoint = \"{token}\"\njwks_uri = \"{jwks}\"\nclient_id = \"{CLIENT_ID}\"\n\
             redirect_uri = \"{console}/auth/callback\"\n\
             end_session_endpoint = \"{end_session}\"\npost_logout_redirect_uri = \"{console}/\"\n",
            site = toml::Value::String(site.display().to_string()),
            issuer = provider.issuer(),
            authorization = provider.authorization_endpoint(),
            token = provider.token_endpoint(),
            jwks = provider.jwks_uri(),
            end_session = provider.end_session_endpoint(),
        );
        let settings =
            ferrobridge_console::config::Config::from_sources(Some(&text), &BTreeMap::new())?
                .resolve()?;
        let state = ferrobridge_console::server::ConsoleState::new(settings)?;
        let cookie = state
            .sessions()
            .cookie_name(ferrobridge_console::session::COOKIE);
        tokio::spawn(ferrobridge_console::server::serve(
            listener,
            ferrobridge_console::server::router(state),
            std::future::pending(),
        ));
        Ok(Self {
            _cdr: cdr,
            tree,
            provider,
            console,
            cookie,
            access_token,
        })
    }

    /// The operator's access token, which the console keeps on its server.
    pub(crate) fn access_token(&self) -> &str {
        &self.access_token
    }

    /// The URL of `path` on the console.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.console)
    }

    /// The test OpenID Provider.
    pub(crate) fn provider(&self) -> &Provider {
        &self.provider
    }

    /// The name of the console's session cookie.
    pub(crate) fn cookie(&self) -> &str {
        &self.cookie
    }

    /// The directory the console saves its files into.
    pub(crate) fn console_files(&self) -> PathBuf {
        self.tree.path().join("console")
    }
}

/// The site bundle's directory: [`SITE_ENV`], or the workspace's
/// `target/site`, which must hold the WebAssembly cargo-leptos wrote.
fn site_root() -> Result<PathBuf, Box<dyn Error>> {
    let site = std::env::var_os(SITE_ENV).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/site"),
        PathBuf::from,
    );
    let bundle = site.join("pkg/ferrobridge-console.wasm");
    if !bundle.is_file() {
        return Err(format!(
            "{} holds no site bundle: build it with scripts/release/console-site.sh --release, or name its directory in {SITE_ENV}",
            site.display()
        )
        .into());
    }
    Ok(site)
}

/// A CDR that answers the bridge's reachability probe, the template list,
/// with an empty list, and the template fetch of the diagnosis template the
/// facade starts with (openEHR ITS-REST 1.1.0,
/// `definition-codegen.openapi.yaml`).
async fn cdr() -> MockServer {
    let cdr = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/definition/template/adl1.4"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(b"[]".to_vec(), "application/json"))
        .mount(&cdr)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/definition/template/adl1.4/{}",
            ferrobridge_testkit::fixtures::DIAGNOSE_TEMPLATE_ID
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/xml")
                .set_body_string(ferrobridge_testkit::fixtures::DIAGNOSE_OPT),
        )
        .mount(&cdr)
        .await;
    cdr
}

/// The mapping tree: the provisioned mappings, the template, and the
/// console's empty directory.
fn tree() -> Result<tempfile::TempDir, Box<dyn Error>> {
    let tree = tempfile::tempdir()?;
    let mappings = tree.path().join("mappings");
    let templates = tree.path().join("templates");
    for directory in [&mappings, &templates, &tree.path().join("console")] {
        std::fs::create_dir_all(directory)?;
    }
    std::fs::write(
        mappings.join("ferrobridge_journey_diagnose.context.yml"),
        CONTEXT_FILE,
    )?;
    std::fs::write(
        mappings.join("ferrobridge_journey_diagnose.yml"),
        MODEL_FILE,
    )?;
    std::fs::write(
        templates.join("diagnose.opt"),
        ferrobridge_testkit::fixtures::DIAGNOSE_OPT,
    )?;
    Ok(tree)
}

/// Starts the bridge over the tree at `tree` and the CDR `cdr`, trusting the
/// operator's token `provider` issues, and returns its base URL.
async fn bridge(
    tree: &Path,
    cdr: &MockServer,
    provider: &Provider,
    lanes: Lanes,
) -> Result<String, Box<dyn Error>> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let bridge = format!("http://{}", listener.local_addr()?);
    let [mappings, templates, identity, console] = [
        tree.join("mappings"),
        tree.join("templates"),
        tree.join("identity.redb"),
        tree.join("console"),
    ]
    .map(|file| toml::Value::String(file.display().to_string()));
    let operations = matches!(lanes, Lanes::All);
    let text = format!(
        "[cdr]\nbase_url = \"{cdr}/\"\ntimeout_ms = 2000\n\n[cdr.retry]\nmax_attempts = 1\n\n\
         [mappings]\ndirectory = {mappings}\ntemplates = {templates}\n\n\
         [operations]\nenabled = {operations}\ncomposition_language = \"en\"\ncomposition_territory = \"NL\"\n\n\
         [facade]\nenabled = true\nbase_url = \"{bridge}/fhir\"\nidentity_store = {identity}\n\
         composition_language = \"en\"\ncomposition_territory = \"NL\"\n\n\
         [auth]\nissuer = \"{issuer}\"\naudience = [\"{bridge}/fhir\"]\n\n\
         [operator]\nenabled = true\nmapping_dir = {console}\n",
        cdr = cdr.uri(),
        issuer = provider.issuer(),
    );
    let settings = ferrobridge_server::config::Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve()?;
    // NOTE: no specification governs this: our own design; the binary drives
    // `serve_settings` on its main thread, so the bridge runs on a runtime of
    // its own thread, as the binary does.
    let serving = std::thread::spawn(move || -> anyhow::Result<()> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        runtime.block_on(serve_settings(
            &settings,
            move || async move { Ok(TcpListener::from_std(listener)?) },
            Backoff {
                initial: Duration::from_millis(20),
                max: Duration::from_millis(50),
            },
            std::future::pending(),
        ))
    });
    ready(&bridge, serving).await?;
    Ok(bridge)
}

/// Waits until the bridge at `bridge` answers its readiness probe with
/// `200`, and fails with the bridge's own error when `serving` ends first.
async fn ready(
    bridge: &str,
    serving: std::thread::JoinHandle<anyhow::Result<()>>,
) -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();
    let mut last = String::new();
    for _ in 0..100 {
        if serving.is_finished() {
            let ended = match serving.join() {
                Ok(Ok(())) => String::from("it ended without an error"),
                Ok(Err(error)) => format!("{error:#}"),
                Err(_panic) => String::from("it panicked"),
            };
            return Err(format!("the bridge stopped before it was ready: {ended}").into());
        }
        match client
            .get(format!("{bridge}/health/readiness"))
            .send()
            .await
        {
            Ok(answer) if answer.status() == reqwest::StatusCode::OK => return Ok(()),
            Ok(answer) => last = format!("{}: {}", answer.status(), answer.text().await?),
            Err(error) => last = error.to_string(),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("the bridge never became ready: {last}").into())
}
