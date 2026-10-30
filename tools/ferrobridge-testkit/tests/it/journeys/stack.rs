// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stack a journey runs against, each part on loopback: a `wiremock`
//! CDR, the test OpenID Provider, a running bridge over a synthetic mapping
//! tree with its FHIR facade, operations lane and operator API, and a
//! running console that draws every page and calls that bridge.
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
    /// The bridge's own thread and the sender that stops it.
    bridge: Running,
}

/// A bridge serving on a thread of its own, and the way to stop it.
#[derive(Debug)]
struct Running {
    /// Sending on it starts the bridge's drain.
    stop: tokio::sync::oneshot::Sender<()>,
    /// The bridge's thread, which ends once the drain has.
    thread: std::thread::JoinHandle<anyhow::Result<()>>,
}

impl Stack {
    /// Starts the CDR, the provider, the bridge with every lane, and the
    /// console.
    pub(crate) async fn start() -> Result<Self, Box<dyn Error>> {
        Self::start_with(Lanes::All).await
    }

    /// Starts the stack with the bridge running `lanes`.
    pub(crate) async fn start_with(lanes: Lanes) -> Result<Self, Box<dyn Error>> {
        Self::start_on(lanes, 0).await
    }

    /// Starts the stack for a capture, with the bridge running `lanes` on
    /// the loopback port `port`, so every page that names the bridge's
    /// address draws the same one each run and a capture rewrites only the
    /// images that changed.
    pub(crate) async fn capture(lanes: Lanes, port: u16) -> Result<Self, Box<dyn Error>> {
        Self::start_on(lanes, port).await
    }

    /// Starts the stack with the bridge running `lanes` on the loopback port
    /// `port`, `0` for one the system chooses.
    async fn start_on(lanes: Lanes, port: u16) -> Result<Self, Box<dyn Error>> {
        // NOTE: no specification governs this: our own design; a capture's CDR and
        // provider sit beside its bridge's port, since the pages draw their addresses.
        let cdr = cdr(neighbour(port, 10)?).await?;
        let tree = tree(port)?;
        let provider = match neighbour(port, 20)? {
            0 => Provider::start().await?,
            fixed => Provider::start_on(listen(fixed)?).await?,
        };
        provider.publish_metadata().await;
        let (bridge, running) = bridge(tree.path(), &cdr, &provider, lanes, port).await?;

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let console = format!("http://{}", listener.local_addr()?);
        let access_token = provider.access_token(&format!("{bridge}/fhir"), SCOPE)?;
        provider.approve(CLIENT_ID, &access_token).await;
        let text = format!(
            "[bridge]\nbase_url = \"{bridge}/\"\n\n\
             [session]\nsecure_cookie = false\n\n\
             [oidc]\nissuer = \"{issuer}\"\nauthorization_endpoint = \"{authorization}\"\n\
             token_endpoint = \"{token}\"\njwks_uri = \"{jwks}\"\nclient_id = \"{CLIENT_ID}\"\n\
             redirect_uri = \"{console}/auth/callback\"\n\
             end_session_endpoint = \"{end_session}\"\npost_logout_redirect_uri = \"{console}/\"\n",
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
        let lifecycle = state.lifecycle().clone();
        tokio::spawn(ferrobridge_console::server::serve(
            listener,
            ferrobridge_console::server::router(state),
            lifecycle,
            std::future::pending(),
        ));
        Ok(Self {
            _cdr: cdr,
            tree,
            provider,
            console,
            cookie,
            access_token,
            bridge: running,
        })
    }

    /// Stops the bridge and waits until its thread has ended, so its port is
    /// free for the next stack that asks for it.
    pub(crate) async fn stop(self) -> Result<(), Box<dyn Error>> {
        let Running { stop, thread } = self.bridge;
        if stop.send(()).is_err() {
            return Err("the bridge had stopped already".into());
        }
        match tokio::task::spawn_blocking(move || thread.join()).await? {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(format!("the bridge ended with an error: {error:#}").into()),
            Err(_panic) => Err("the bridge panicked".into()),
        }
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

/// The port `offset` above `port` for a capture, `0` when `port` is `0`.
fn neighbour(port: u16, offset: u16) -> Result<u16, Box<dyn Error>> {
    if port == 0 {
        return Ok(0);
    }
    port.checked_add(offset)
        .ok_or_else(|| format!("no port {offset} above {port}").into())
}

/// A listener on the loopback port `port`, `0` for one the system chooses.
fn listen(port: u16) -> Result<std::net::TcpListener, Box<dyn Error>> {
    std::net::TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("nothing can listen on 127.0.0.1:{port}: {error}").into())
}

/// A CDR that answers the bridge's reachability probe, the template list,
/// with an empty list, and the template fetch of the diagnosis template the
/// facade starts with (openEHR ITS-REST 1.1.0,
/// `definition-codegen.openapi.yaml`), on the loopback port `port`.
async fn cdr(port: u16) -> Result<MockServer, Box<dyn Error>> {
    let cdr = MockServer::builder().listener(listen(port)?).start().await;
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
    Ok(cdr)
}

/// Where a capture keeps its mapping trees: one fixed path outside the
/// checkout, so the pages that draw it look the same from every worktree (no
/// specification governs this: our own design).
const CAPTURE_TREES: &str = "/tmp/ferrobridge-journey-capture";

/// The mapping tree: the provisioned mappings, the template, and the
/// console's empty directory. A capture's tree (`port` not `0`) is
/// [`CAPTURE_TREES`]`/bridge-<port>`, the same path each run and in every
/// checkout, since the configuration and diagnostics pages draw it; every
/// other tree is a fresh temporary directory.
fn tree(port: u16) -> Result<tempfile::TempDir, Box<dyn Error>> {
    let tree = if port == 0 {
        tempfile::tempdir()?
    } else {
        let base = Path::new(CAPTURE_TREES).to_path_buf();
        let name = format!("bridge-{port}");
        std::fs::create_dir_all(&base)?;
        let base = base.canonicalize()?;
        match std::fs::remove_dir_all(base.join(&name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        tempfile::Builder::new()
            .prefix(&name)
            .rand_bytes(0)
            .tempdir_in(&base)?
    };
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

/// Starts the bridge over the tree at `tree` and the CDR `cdr` on the
/// loopback port `port` (`0` for one the system chooses), trusting the
/// operator's token `provider` issues, and returns its base URL.
async fn bridge(
    tree: &Path,
    cdr: &MockServer,
    provider: &Provider,
    lanes: Lanes,
    port: u16,
) -> Result<(String, Running), Box<dyn Error>> {
    let listener = listen(port)?;
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
         [operator]\nenabled = true\nmapping_dir = {console}\n\n\
         [usage_report]\nenabled = false\n",
        cdr = cdr.uri(),
        issuer = provider.issuer(),
    );
    let settings = ferrobridge_server::config::Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve()?;
    // NOTE: no specification governs this: our own design; the binary drives
    // `serve_settings` on its main thread, so the bridge runs on a runtime of
    // its own thread, as the binary does.
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
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
            async move {
                // NOTE: no specification governs this: our own design; a dropped
                // sender stops the bridge as a sent stop does.
                let _sent_or_dropped = stopped.await;
            },
        ))
    });
    let thread = ready(&bridge, serving).await?;
    Ok((bridge, Running { stop, thread }))
}

/// Waits until the bridge at `bridge` answers its readiness probe with
/// `200` and hands its thread `serving` back, and fails with the bridge's own
/// error when `serving` ends first.
async fn ready(
    bridge: &str,
    serving: std::thread::JoinHandle<anyhow::Result<()>>,
) -> Result<std::thread::JoinHandle<anyhow::Result<()>>, Box<dyn Error>> {
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
            Ok(answer) if answer.status() == reqwest::StatusCode::OK => return Ok(serving),
            Ok(answer) => last = format!("{}: {}", answer.status(), answer.text().await?),
            Err(error) => last = error.to_string(),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("the bridge never became ready: {last}").into())
}
