// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroBRIDGE server library: the run path the `ferrobridge` binary and
//! its integration tests share.
//!
//! [`cli`] parses the command line, [`config`] reads the file and the
//! environment into one [`config::Settings`], [`telemetry`] installs the
//! subscriber, [`router`] builds the HTTP surface over [`state::AppState`],
//! and [`serve`] runs it on a bound listener until the process is asked to
//! stop. `main.rs` only hands in the arguments and returns the exit code.
#![doc(test(attr(deny(warnings))))]

pub mod auth;
pub mod banner;
pub mod build_info;
pub mod cdr;
pub mod cli;
pub mod config;
pub mod etl;
pub mod facade;
pub mod health;
pub mod hl7v2;
pub mod indicators;
mod jobs;
pub mod mappings;
pub mod operations;
pub mod panic;
pub mod request_id;
pub mod request_log;
pub mod startup;
pub mod state;
pub mod telemetry;
pub mod walk;

use std::future::Future;
use std::io::IsTerminal;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use clap::Parser;
use http::StatusCode;
use tokio::net::TcpListener;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;

use crate::cli::Cli;
use crate::config::{Config, ServerSettings, Settings};
use crate::state::AppState;

/// The exit code of a job that parses and does not run yet.
///
/// Two is the conventional usage exit, the code a command line uses when the
/// invocation is understood and refused.
pub const EXIT_UNAVAILABLE_JOB: u8 = 2;

/// The exit code of a refused configuration.
///
/// `EX_CONFIG` from `sysexits`
/// (<https://man.freebsd.org/cgi/man.cgi?query=sysexits>), so an orchestrator
/// tells a bad configuration from a failure to serve.
pub const EXIT_CONFIG: u8 = 78;

/// Runs the binary with `args` and returns the process exit code.
///
/// `args` is the whole argument vector, the program name included, so the
/// command-line parser reports usage under the right name.
#[must_use]
#[expect(
    clippy::print_stderr,
    reason = "a usage refusal and a refused configuration are reported before any log subscriber exists"
)]
pub fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => return ExitCode::from(clap_exit(&error)),
    };
    let job = match (&cli.command, cli.command.pending_issue()) {
        (cli::Command::Serve, None) => Job::Serve,
        (
            cli::Command::Cdm {
                command: cli::Cdm::Init { with_constraints },
            },
            None,
        ) => Job::CdmInit {
            with_constraints: *with_constraints,
        },
        (
            cli::Command::Etl {
                command: cli::Etl::Run { resume, since },
            },
            None,
        ) => Job::EtlRun(etl::RunOptions {
            resume: *resume,
            since: since.clone(),
        }),
        (command, issue) => {
            let issue = issue.map_or_else(String::new, |issue| {
                format!("; it lands with issue #{issue}")
            });
            eprintln!(
                "ferrobridge: `{}` is not implemented yet{issue}",
                command.spelling()
            );
            return ExitCode::from(EXIT_UNAVAILABLE_JOB);
        }
    };
    let settings = match Config::load(cli.config.as_deref()).and_then(|config| config.resolve()) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("ferrobridge: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let stdout_is_terminal = std::io::stdout().is_terminal();
    let format = settings.telemetry.format;
    if matches!(job, Job::Serve)
        && format.resolve(stdout_is_terminal) == telemetry::Rendering::Pretty
    {
        banner::print(
            &startup::lanes(&settings),
            format.colour(stdout_is_terminal),
        );
    }
    if let Err(error) = telemetry::init(
        settings.telemetry.format,
        &settings.telemetry.filter,
        stdout_is_terminal,
    ) {
        eprintln!("ferrobridge: cannot start: {}", chain(&error));
        return ExitCode::from(EXIT_CONFIG);
    }
    match job {
        Job::Serve => match serve_command(settings) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                tracing::error!(error = format!("{error:#}"), "cannot serve");
                ExitCode::FAILURE
            }
        },
        Job::CdmInit { with_constraints } => {
            let Some(cdm) = settings.cdm else {
                eprintln!("ferrobridge: cannot start: `cdm init` needs a [cdm] section");
                return ExitCode::from(EXIT_CONFIG);
            };
            match jobs::cdm_init_command(&cdm, with_constraints) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    tracing::error!(error = format!("{error:#}"), "cdm init failed");
                    ExitCode::FAILURE
                }
            }
        }
        Job::EtlRun(options) => {
            if let Some(missing) = etl::job::missing_section(&settings) {
                eprintln!("ferrobridge: cannot start: `etl run` needs {missing}");
                return ExitCode::from(EXIT_CONFIG);
            }
            match jobs::etl_run_command(&settings, &options) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    tracing::error!(error = format!("{error:#}"), "etl run failed");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

/// The jobs the binary runs today.
enum Job {
    /// `serve`.
    Serve,
    /// `cdm init`.
    CdmInit {
        /// Whether OHDSI's foreign keys follow the tables.
        with_constraints: bool,
    },
    /// `etl run`.
    EtlRun(etl::RunOptions),
}

/// Builds the runtime and serves until the process is asked to stop.
fn serve_command(settings: Settings) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        use anyhow::Context;

        build_info::log();
        let mut state = AppState::build(&settings)
            .await
            .context("building the upstream clients")?;
        if let Some(auth) = settings.auth.clone() {
            let authority = auth::Authority::discover(auth)
                .await
                .context("reading the authorization server")?;
            state = state.with_auth(Arc::new(authority));
        } else {
            tracing::warn!(
                "[auth] is off: the FHIR facade authenticates no caller and serves every request"
            );
        }
        let mut lanes = startup::lanes(&settings);
        if let Some(lane) = settings.facade.as_ref() {
            let (mounted, counts) = build_facade(&settings, lane)
                .await
                .context("starting the FHIR facade")?;
            state = state.with_facade(Arc::new(mounted));
            set_counts(&mut lanes, "facade", counts);
        }
        if let Some(operations) = state.operations() {
            set_counts(&mut lanes, "operations", operation_counts(operations));
        }
        let mut face = None;
        if let Some(lane) = settings.hl7v2.as_ref() {
            let (started, listening) = start_hl7v2(&settings, lane, &state)
                .await
                .context("starting the HL7 v2 face")?;
            state = state.with_hl7v2(listening);
            face = Some(started);
        }
        startup::log(&lanes);
        let state = Arc::new(state.with_lanes(lanes));
        tracing::info!(
            version = state::VERSION,
            indicators = state.health().names().join(","),
            "ferrobridge starting"
        );
        let listener = TcpListener::bind(settings.server.listen)
            .await
            .with_context(|| format!("binding {}", settings.server.listen))?;
        tracing::info!(listen = %settings.server.listen, "listening");
        let app = router(Arc::clone(&state), &settings.server);
        match face {
            None => serve(listener, app, &settings.server)
                .await
                .context("serving HTTP")?,
            Some(face) => serve_with_face(listener, app, &settings.server, face).await?,
        }
        tracing::info!("ferrobridge stopped");
        Ok(())
    })
}

/// The HL7 v2 face, bound and ready to serve.
struct StartedFace {
    /// The bound MLLP socket.
    socket: TcpListener,
    /// The face its frames are answered by.
    face: Arc<hl7v2::Face>,
    /// How the listener runs.
    listener: hl7v2::Listener,
    /// Whether the listener accepts, for the readiness probe.
    listening: hl7v2::Listening,
}

/// Loads the HL7 v2 face's `ConceptMaps` and binds its listener.
///
/// A corpus that does not load and a port that cannot be bound both refuse
/// the start, as a facade mapping that does not compile does.
async fn start_hl7v2(
    settings: &Settings,
    lane: &config::Hl7v2Settings,
    state: &AppState,
) -> anyhow::Result<(StartedFace, hl7v2::Listening)> {
    use anyhow::Context;

    let facade = state
        .facade()
        .cloned()
        .context("the HL7 v2 face writes through the facade, and the facade is off")?;
    let corpus = hl7v2::load_corpus(lane).with_context(|| {
        format!(
            "loading the v2-to-FHIR ConceptMaps from {}",
            lane.concept_maps.display()
        )
    })?;
    let maps = corpus.maps().count();
    let terminology = settings
        .terminology
        .as_ref()
        .map(|config| ferrobridge_term::client::Client::new(config.clone()))
        .transpose()
        .context("building the terminology client the HL7 v2 face translates through")?;
    let socket = TcpListener::bind(lane.listen)
        .await
        .with_context(|| format!("binding {}", lane.listen))?;
    tracing::info!(
        lane = "hl7v2",
        listen = %lane.listen,
        concept_maps = maps,
        profiles = lane.profiles.len(),
        "the HL7 v2 face listens for MLLP"
    );
    let listening = hl7v2::Listening::default();
    let face = StartedFace {
        socket,
        face: Arc::new(hl7v2::Face::new(facade, corpus, terminology, lane)),
        listener: hl7v2::Listener::of(lane, settings.server.shutdown_timeout),
        listening: listening.clone(),
    };
    Ok((face, listening))
}

/// Serves HTTP and the HL7 v2 face until the process is asked to stop, each
/// draining within the same bound.
///
/// The one signal stops both, and a listener that fails stops the other, so
/// the process never keeps half its surface.
async fn serve_with_face(
    listener: TcpListener,
    app: Router,
    server: &ServerSettings,
    face: StartedFace,
) -> anyhow::Result<()> {
    use anyhow::Context;

    let (stop, _) = tokio::sync::watch::channel(false);
    let stop = Arc::new(stop);
    let signal = {
        let stop = Arc::clone(&stop);
        tokio::spawn(async move {
            shutdown_signal().await;
            stop.send_replace(true);
        })
    };
    let http = async {
        let ended = serve_until(listener, app, server.shutdown_timeout, stopped(&stop)).await;
        stop.send_replace(true);
        ended
    };
    let mllp = async {
        let ended = hl7v2::serve_until(
            face.socket,
            face.face,
            face.listener,
            face.listening,
            stopped(&stop),
        )
        .await;
        stop.send_replace(true);
        ended
    };
    let (http, mllp) = tokio::join!(http, mllp);
    signal.abort();
    http.context("serving HTTP")?;
    mllp.context("serving MLLP")?;
    Ok(())
}

/// Completes once `stop` reads `true`.
fn stopped(stop: &tokio::sync::watch::Sender<bool>) -> impl Future<Output = ()> + Send + 'static {
    let mut receiver = stop.subscribe();
    async move {
        let _settled = receiver.wait_for(|stopped| *stopped).await.is_ok();
    }
}

/// Records `counts` on the lane called `name`.
fn set_counts(lanes: &mut [startup::Lane], name: &str, counts: startup::MappingCounts) {
    if let Some(lane) = lanes.iter_mut().find(|lane| lane.name == name) {
        lane.mappings = Some(counts);
    }
}

/// Returns what the operations lane compiled: one program per context, over
/// the distinct templates the programs name.
fn operation_counts(lane: &state::OperationsLane) -> startup::MappingCounts {
    let programs = lane.programs().programs();
    let contexts: std::collections::BTreeSet<&str> = programs
        .iter()
        .map(|program| program.context().as_str())
        .collect();
    let templates: std::collections::BTreeSet<&str> = programs
        .iter()
        .map(|program| program.template().id().as_str())
        .collect();
    startup::MappingCounts {
        contexts: contexts.len(),
        programs: programs.len(),
        templates: templates.len(),
    }
}

/// Builds the FHIR facade this deployment mounts and returns it with what its
/// mapping set loaded.
///
/// The mapping set is read and compiled once, at boot, against the templates
/// the CDR holds, so a mapping that does not compile refuses the start rather
/// than the first request that touches it (`docs/architecture.md` §4.3). The
/// identity store is opened here too, so a path that cannot be written is a
/// boot error.
async fn build_facade(
    settings: &Settings,
    lane: &config::FacadeSettings,
) -> anyhow::Result<(facade::Facade, startup::MappingCounts)> {
    use anyhow::Context;

    let cdr = settings
        .cdr
        .as_ref()
        .context("the facade needs a [cdr] section: it maps every request onto CDR operations")?;
    let client =
        cdr::CdrClient::new(cdr).context("building the CDR client the facade calls through")?;
    let directory = settings
        .mapping_directory
        .as_ref()
        .context("the facade needs [mappings] directory: it runs the mappings it finds there")?;
    let set = facade::programs::read_set(directory).context("reading the mapping set")?;
    let templates = facade::programs::fetch_templates(&set, &client)
        .await
        .context("fetching the templates the mappings name")?;
    let programs =
        facade::programs::compile_set(&set, &templates).context("compiling the mapping set")?;
    tracing::info!(
        lane = "facade",
        types = programs
            .resource_types()
            .iter()
            .cloned()
            .collect::<Vec<String>>()
            .join(","),
        "the FHIR facade serves these resource types"
    );
    let counts = startup::MappingCounts {
        contexts: set.contexts().count(),
        programs: programs.len(),
        templates: templates.len(),
    };
    let search = facade::search::load(directory, &programs).context("reading the searches")?;
    let store = facade::identity::redb_store::RedbStore::open(&lane.identity_store)
        .with_context(|| format!("opening {}", lane.identity_store.display()))?;
    let built = facade::Facade::new(programs, Arc::new(store), client, lane.settings.clone());
    Ok((built.with_search(search, lane.search), counts))
}

/// Returns the exit code a command-line refusal deserves.
///
/// `--help` and `--version` are not failures: clap reports both as an error
/// whose kind says the text was printed
/// (<https://docs.rs/clap/4/clap/error/enum.ErrorKind.html>).
#[expect(
    clippy::print_stdout,
    reason = "clap renders help and version to stdout, which is where a person reads them"
)]
#[expect(
    clippy::print_stderr,
    reason = "a usage refusal is reported before any log subscriber exists"
)]
fn clap_exit(error: &clap::Error) -> u8 {
    if error.use_stderr() {
        eprint!("{error}");
        EXIT_UNAVAILABLE_JOB
    } else {
        print!("{error}");
        0
    }
}

/// Returns `error` and every cause behind it as one line.
fn chain(error: &dyn std::error::Error) -> String {
    let mut line = error.to_string();
    let mut cause = error.source();
    while let Some(source) = cause {
        line.push_str(": ");
        line.push_str(&source.to_string());
        cause = source.source();
    }
    line
}

/// Builds the HTTP application over `state`, with the shared middleware.
///
/// `GET /` answers a small JSON document naming the product and its version,
/// `GET /health/liveness` answers `200` while the process is up, and
/// `GET /health/readiness` answers `200` when every registered indicator is up
/// and `503` with each indicator's state otherwise, and `GET /health/info`
/// answers the build facts and the pins as JSON. The two FHIRconnect
/// operations and their direct forms are mounted under `/fhir`. Every route
/// passes [`auth::guard::guard`], which checks the bearer token of a FHIR
/// request when the state carries a resource server and leaves the facade
/// open when it does not.
pub fn router(state: Arc<AppState>, server: &ServerSettings) -> Router {
    let mut routes = Router::new()
        .route("/", get(root))
        .route("/health/liveness", get(liveness))
        .route("/health/readiness", get(readiness))
        .route("/health/info", get(info))
        .with_state(Arc::clone(&state))
        .merge(operations::router(Arc::clone(&state)));
    if let Some(authority) = state.auth() {
        routes = routes.merge(auth::smart::routes(authority));
    }
    if let Some(mounted) = state.facade() {
        let shape = facade::capability::Shape {
            operations: if state.operations().is_some() {
                facade::capability::Operations::Served
            } else {
                facade::capability::Operations::Absent
            },
            hl7v2: if state.hl7v2().is_some() {
                facade::capability::Hl7v2::Configured
            } else {
                facade::capability::Hl7v2::Absent
            },
            security: state
                .auth()
                .map(|authority| auth::smart::security(authority)),
        };
        routes = routes.merge(facade::routes(Arc::clone(mounted), shape));
    }
    let gate = state
        .auth()
        .map_or_else(auth::guard::Gate::open, |authority| {
            auth::guard::Gate::new(Arc::clone(authority))
        });
    let routes = routes.layer(axum::middleware::from_fn_with_state(
        gate,
        auth::guard::guard,
    ));
    with_middleware(routes, state, server)
}

/// Applies the middleware stack every FerroBRIDGE surface carries to `router`.
///
/// Outermost first: the request-id normalizer, the layer that mints one, the
/// panic renderer, the layer that propagates the id onto the response, the
/// request log, the panic catcher, the FHIR edge renderer, the request timeout
/// and the body-size ceiling. The renderer sits outside the propagate layer
/// because it reads the id from the response the propagate layer has just
/// stamped, and it answers a `/fhir` route with an `OperationOutcome`
/// ([`panic::render`]). The edge renderer turns the timeout's and the ceiling's answer on
/// a `/fhir` route into an `OperationOutcome` ([`facade::edge`]). The log sits
/// outside the catcher, the timeout and the ceiling so a caught panic, a
/// timeout and a `413` each leave their one request line like every other
/// outcome.
pub fn with_middleware(router: Router, state: Arc<AppState>, server: &ServerSettings) -> Router {
    router
        .layer(RequestBodyLimitLayer::new(server.body_limit))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            server.request_timeout,
        ))
        .layer(axum::middleware::from_fn_with_state(
            facade::edge::Edge::new(server.body_limit, server.request_timeout),
            facade::edge::render,
        ))
        .layer(CatchPanicLayer::custom(panic::caught))
        .layer(axum::middleware::from_fn_with_state(
            state,
            request_log::log,
        ))
        .layer(PropagateRequestIdLayer::new(request_id::HEADER))
        .layer(axum::middleware::from_fn(panic::render))
        .layer(SetRequestIdLayer::new(request_id::HEADER, request_id::Mint))
        .layer(axum::middleware::map_request(request_id::strip_illegal))
}

/// `GET /`: the product and the version, as JSON.
async fn root() -> Response {
    axum::Json(serde_json::json!({
        "product": state::PRODUCT,
        "version": state::VERSION,
    }))
    .into_response()
}

/// `GET /health/liveness`: `200` while the process is up.
async fn liveness() -> StatusCode {
    StatusCode::OK
}

/// `GET /health/info`: the build facts, the pins and the lanes, as JSON.
async fn info(State(state): State<Arc<AppState>>) -> Response {
    axum::Json(state.info()).into_response()
}

/// `GET /health/readiness`: the state of every registered indicator.
async fn readiness(State(state): State<Arc<AppState>>) -> Response {
    let report = state.health().evaluate().await;
    (report.status(), axum::Json(report)).into_response()
}

/// Serves `app` on an already-bound listener until the process receives
/// `SIGTERM` or `SIGINT`, then drains.
///
/// # Errors
/// Returns the I/O error from accepting or serving connections.
pub async fn serve(
    listener: TcpListener,
    app: Router,
    server: &ServerSettings,
) -> std::io::Result<()> {
    serve_until(listener, app, server.shutdown_timeout, shutdown_signal()).await
}

/// Serves `app` on an already-bound listener until `shutdown` completes, then
/// finishes the requests in flight within `drain`.
///
/// A container runtime stops a container with `SIGTERM` to PID 1 and kills it
/// after a grace period, so the drain is bounded here too: a connection still
/// open when `drain` elapses is dropped and the function returns.
///
/// # Errors
/// Returns the I/O error from accepting or serving connections.
pub async fn serve_until<F>(
    listener: TcpListener,
    app: Router,
    drain: Duration,
    shutdown: F,
) -> std::io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let signalled = Arc::new(tokio::sync::Notify::new());
    let inner = Arc::clone(&signalled);
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.await;
            inner.notify_one();
        })
        .into_future();
    let mut server = std::pin::pin!(server);
    tokio::select! {
        result = &mut server => return result,
        () = signalled.notified() => {}
    }
    if let Ok(result) = tokio::time::timeout(drain, server).await {
        return result;
    }
    tracing::warn!(
        drain_ms = drain.as_millis(),
        "the drain did not finish in time; the remaining connections are dropped"
    );
    Ok(())
}

/// Completes when the process receives `SIGTERM` or `SIGINT`.
///
/// A failure to install a handler is logged and that arm never completes, so
/// the server keeps serving and the runtime's own kill stays the backstop.
pub async fn shutdown_signal() {
    let interrupt = async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {}
            Err(error) => {
                tracing::error!(%error, "cannot listen for SIGINT");
                std::future::pending::<()>().await;
            }
        }
    };
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    tokio::select! {
        () = interrupt => tracing::info!("SIGINT received, draining"),
        () = terminate => tracing::info!("SIGTERM received, draining"),
    }
}
