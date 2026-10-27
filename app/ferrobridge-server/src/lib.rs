// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroBRIDGE server library: the run path the `ferrobridge` binary and
//! its integration tests share.
//!
//! [`cli`] parses the command line, [`config`] reads the file, the
//! environment and the `--set` pairs into one [`config::Settings`],
//! `ferrohealth-telemetry` installs the console, [`router`] builds the HTTP
//! surface over [`state::AppState`] inside the stack of `ferrohealth-server`,
//! and [`serve_until`] runs it on a bound listener until the process is asked
//! to stop. `main.rs` only hands in the arguments and returns the exit code.
#![doc(test(attr(deny(warnings))))]

pub mod banner;
pub mod boot;
pub mod build_info;
pub mod cdr;
pub mod check;
pub mod cli;
pub mod config;
pub mod credentials;
pub mod etl;
pub mod facade;
pub mod hl7v2;
pub mod indicators;
mod jobs;
mod lanes;
pub mod mappings;
pub mod operations;
pub mod operator;
pub mod panic;
pub mod quality;
pub mod smart;
pub mod startup;
pub mod state;
pub mod terminology;
pub mod usage_report;
pub mod walk;

use std::future::Future;
use std::io::IsTerminal;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use clap::Parser;
use ferrohealth_config::loader::Loaded;
use ferrohealth_server::cli::{Common, ConfigCommand};
use ferrohealth_server::health::Health;
use ferrohealth_server::healthcheck::Outcome;
use ferrohealth_server::section::ServerSection;
use ferrohealth_server::serve::{Drained, ServeError};
use ferrohealth_server::stack::{self, StackPolicy};
use ferrohealth_telemetry::request_log::{RequestId, RequestLog};
use ferrohealth_telemetry::telemetry::{Console, Service, Telemetry};
use http::HeaderValue;
use tokio::net::TcpListener;

use crate::cli::Cli;
use crate::config::{Config, Settings};
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

/// The exit code of a `mapping check` that could not run: a directory it
/// cannot read, or a CDR that does not serve a template.
///
/// `EX_UNAVAILABLE` from `sysexits`, so a pipeline tells a check that did
/// not run from a set that failed it, which exits 1.
pub const EXIT_CHECK_UNAVAILABLE: u8 = 69;

/// How long `healthcheck` waits for the running server's answer.
pub const HEALTHCHECK_TIMEOUT: Duration = Duration::from_secs(3);

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
    let job = match Job::of(&cli.command) {
        Ok(job) => job,
        Err(code) => return code,
    };
    let loaded = match Config::load(cli.config.config.as_deref(), &cli.config.sets) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("ferrobridge: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    match job {
        Job::ConfigCheck => return config_check(loaded),
        Job::Healthcheck => return healthcheck_command(&loaded.settings().server),
        _ => {}
    }
    let config = Config::from_loaded(loaded);
    let resolved = match job {
        Job::Serve => config.resolve(),
        _ => config.resolve_job(),
    };
    let settings = match resolved {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("ferrobridge: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let console = Console::detect();
    if matches!(job, Job::Serve) {
        banner::print(
            &startup::lanes(&settings),
            settings
                .telemetry
                .format
                .resolve(std::io::stdout().is_terminal()),
        );
    }
    // NOTE: no specification governs this: our own design; `mapping check`
    // installs no log subscriber, so its report is the whole of stdout.
    if matches!(job, Job::MappingCheck { .. }) {
        return job.run(settings);
    }
    let service = Service::new("ferrobridge", state::VERSION);
    let telemetry = match Telemetry::new(&settings.telemetry, service) {
        Ok(telemetry) => telemetry,
        Err(error) => {
            eprintln!("ferrobridge: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    if let Err(error) = telemetry.install(&settings.telemetry, &console) {
        eprintln!("ferrobridge: cannot start: {}", chain(&error));
        return ExitCode::from(EXIT_CONFIG);
    }
    ferrohealth_telemetry::panic::install_hook();
    let terminal = std::io::stdout().is_terminal();
    tracing::info!(
        format = match settings.telemetry.format.resolve(terminal) {
            ferrohealth_telemetry::section::Rendering::Json => "json",
            ferrohealth_telemetry::section::Rendering::Pretty => "pretty",
        },
        colour = console.colour(settings.telemetry.format),
        filter = settings.telemetry.filter.as_str(),
        "console"
    );
    let code = job.run(settings);
    if let Err(error) = telemetry.shutdown() {
        tracing::warn!(error = %error, "the telemetry did not flush");
    }
    code
}

/// `ferrobridge config check`: the running tree with every secret redacted
/// and where each key came from, then every refusal the resolution finds.
#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "config check prints the running tree for a person and its refusals on stderr"
)]
fn config_check(loaded: Loaded<Config>) -> ExitCode {
    let report = match ferrohealth_config::report::check_report(&loaded) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("ferrobridge config check: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    println!("{report}");
    match Config::from_loaded(loaded).resolve() {
        Ok(_settings) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ferrobridge config check: {}", chain(&error));
            ExitCode::from(EXIT_CONFIG)
        }
    }
}

/// `ferrobridge healthcheck`: silent when the server beside this process is
/// ready, the reason on stderr otherwise.
#[expect(
    clippy::print_stderr,
    reason = "the probe's refusal is the whole of its output, and no log subscriber exists"
)]
fn healthcheck_command(section: &ServerSection) -> ExitCode {
    let outcome = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(healthcheck(section)),
        Err(error) => Outcome::Failed(format!("the probe could not start: {error}")),
    };
    if outcome.exit_code() == 0 {
        return ExitCode::SUCCESS;
    }
    eprintln!("ferrobridge healthcheck: {outcome}");
    ExitCode::FAILURE
}

/// Asks the server `section` configures for its readiness over loopback.
pub async fn healthcheck(section: &ServerSection) -> Outcome {
    use ferrohealth_server::healthcheck::{check, check_tls, target};
    let address = target(section.listen);
    let base = section.base_path.as_str();
    match &section.tls {
        None => check(address, base, HEALTHCHECK_TIMEOUT).await,
        Some(tls) => match ferrohealth_server::tls::HealthcheckTls::read(tls) {
            Ok(pinned) => check_tls(address, base, HEALTHCHECK_TIMEOUT, &pinned).await,
            Err(error) => Outcome::Failed(format!("the TLS files do not read: {error}")),
        },
    }
}

impl Job {
    /// Returns the job `command` names, or the exit code of one that parses
    /// and does not run yet.
    #[expect(
        clippy::print_stderr,
        reason = "a pending job is reported before any log subscriber exists"
    )]
    fn of(command: &cli::Command) -> Result<Self, ExitCode> {
        Ok(match (command, command.pending_issue()) {
            (cli::Command::Common(Common::Serve), None) => Self::Serve,
            (
                cli::Command::Common(Common::Config {
                    command: ConfigCommand::Check,
                }),
                None,
            ) => Self::ConfigCheck,
            (cli::Command::Common(Common::Healthcheck), None) => Self::Healthcheck,
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
            (
                cli::Command::Vocab {
                    command: cli::Vocab::Load { directory, schema },
                },
                None,
            ) => Job::VocabLoad {
                directory: directory.clone(),
                schema: schema.clone(),
            },
            (
                cli::Command::Etl {
                    command: cli::Etl::Fhir { input },
                },
                None,
            ) => Job::EtlFhir {
                input: input.clone(),
            },
            (
                cli::Command::Mapping {
                    command: cli::Mapping::Check { format, vocabulary },
                },
                None,
            ) => Job::MappingCheck {
                format: *format,
                vocabulary: *vocabulary,
            },
            (
                cli::Command::Dq {
                    command: cli::Dq::Check { format },
                },
                None,
            ) => Job::DqCheck { format: *format },
            (command, issue) => {
                let issue = issue.map_or_else(String::new, |issue| {
                    format!("; it lands with issue #{issue}")
                });
                eprintln!(
                    "ferrobridge: `{}` is not implemented yet{issue}",
                    command.spelling()
                );
                return Err(ExitCode::from(EXIT_UNAVAILABLE_JOB));
            }
        })
    }

    /// Runs the job under `settings` and returns the process exit code.
    #[expect(
        clippy::print_stderr,
        reason = "a missing section and a refused check are reported on stderr, outside the report on stdout"
    )]
    fn run(self, settings: Settings) -> ExitCode {
        match self {
            Job::ConfigCheck | Job::Healthcheck => ExitCode::SUCCESS,
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
            Job::VocabLoad { directory, schema } => {
                let Some(cdm) = settings.cdm else {
                    eprintln!("ferrobridge: cannot start: `vocab load` needs a [cdm] section");
                    return ExitCode::from(EXIT_CONFIG);
                };
                match jobs::vocab_load_command(&cdm, &directory, &schema) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(error) => {
                        tracing::error!(error = format!("{error:#}"), "vocab load failed");
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
            Job::DqCheck { format } => {
                let Some(cdm) = settings.cdm else {
                    eprintln!("ferrobridge: cannot start: `dq check` needs a [cdm] section");
                    return ExitCode::from(EXIT_CONFIG);
                };
                match jobs::dq_check_command(&cdm, format) {
                    Ok(true) => ExitCode::SUCCESS,
                    Ok(false) => ExitCode::FAILURE,
                    Err(error) => {
                        tracing::error!(error = format!("{error:#}"), "dq check failed");
                        ExitCode::from(EXIT_CHECK_UNAVAILABLE)
                    }
                }
            }
            Job::EtlFhir { input } => {
                if let Some(missing) = etl::job::missing_fhir_section(&settings) {
                    eprintln!("ferrobridge: cannot start: `etl fhir` needs {missing}");
                    return ExitCode::from(EXIT_CONFIG);
                }
                match jobs::etl_fhir_command(&settings, &input) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(error) => {
                        tracing::error!(error = format!("{error:#}"), "etl fhir failed");
                        ExitCode::FAILURE
                    }
                }
            }
            Job::MappingCheck { format, vocabulary } => {
                match jobs::mapping_check_command(&settings, format, vocabulary) {
                    Ok(true) => ExitCode::SUCCESS,
                    Ok(false) => ExitCode::FAILURE,
                    Err(jobs::CheckFailure::Configuration(error)) => {
                        eprintln!("ferrobridge: cannot check: {}", chain(&error));
                        ExitCode::from(EXIT_CONFIG)
                    }
                    Err(jobs::CheckFailure::Run(error)) => {
                        eprintln!("ferrobridge: cannot check: {error:#}");
                        ExitCode::from(EXIT_CHECK_UNAVAILABLE)
                    }
                }
            }
        }
    }
}

/// The jobs the binary runs today.
enum Job {
    /// `serve`.
    Serve,
    /// `config check`.
    ConfigCheck,
    /// `healthcheck`.
    Healthcheck,
    /// `cdm init`.
    CdmInit {
        /// Whether OHDSI's foreign keys follow the tables.
        with_constraints: bool,
    },
    /// `vocab load`.
    VocabLoad {
        /// The directory holding the download.
        directory: std::path::PathBuf,
        /// The schema the vocabulary tables live in.
        schema: String,
    },
    /// `etl run`.
    EtlRun(etl::RunOptions),
    /// `dq check`.
    DqCheck {
        /// How the result is written.
        format: cli::ReportFormat,
    },
    /// `etl fhir`.
    EtlFhir {
        /// The FHIR input.
        input: std::path::PathBuf,
    },
    /// `mapping check`.
    MappingCheck {
        /// How the report is written.
        format: cli::ReportFormat,
        /// Whether the OMOCL literals are checked against the vocabulary.
        vocabulary: bool,
    },
}

/// Builds the runtime and serves until the process is asked to stop.
fn serve_command(settings: Settings) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        use anyhow::Context;

        build_info::log();
        let listen = settings.server.listen;
        let bind = move || async move {
            TcpListener::bind(listen)
                .await
                .with_context(|| format!("binding {listen}"))
        };
        boot::serve_settings(
            &settings,
            bind,
            boot::Backoff::default(),
            ferrohealth_server::serve::shutdown_signal(),
        )
        .await?;
        tracing::info!("ferrobridge stopped");
        Ok(())
    })
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

/// Builds the HTTP application over `state` inside the family's stack.
///
/// `GET /` answers a small JSON document naming the product and its version,
/// and the stack serves `/health`, `/health/liveness`, `/health/readiness`
/// and `/health/info` over the state's indicators. The two FHIRconnect
/// operations and their direct forms are mounted under `/fhir`. Every route
/// passes the layer of [`facade::guard::layer`], which checks the credential
/// of a request under `/fhir` when the state carries a resource server and
/// leaves the facade open when it does not.
pub fn router(state: Arc<AppState>, server: &ServerSection) -> Router {
    let mut routes = Router::new()
        .route("/", get(root))
        .merge(operations::router(Arc::clone(&state)));
    if let Some(smart) = state.auth() {
        routes = routes.merge(smart::routes(smart));
        if let Some(operator) = state.operator() {
            routes = routes.merge(operator::routes(
                Arc::clone(operator),
                smart,
                state.health().clone(),
                state.lifecycle().clone(),
            ));
        }
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
            security: state.auth().map(|smart| smart.security()),
        };
        let operator = facade::guard::OperatorScope(
            state.auth().map(|smart| smart.operator_scope().to_owned()),
        );
        routes = routes.merge(facade::routes(Arc::clone(mounted), shape, operator));
    }
    let routes = routes.layer(facade::guard::layer(state.auth().map(AsRef::as_ref)));
    with_middleware(routes, state, server)
}

/// Wraps `product` in the stack every FerroBRIDGE surface carries.
///
/// The stack is `ferrohealth-server`'s request id, panic catch, timeout, body
/// limit, compression and health routes, and `ferrohealth-telemetry`'s
/// request log feeding the usage report's window.
///
/// A panic and a refusal the stack writes on a `/fhir` route answer an
/// `OperationOutcome` ([`panic::body`], [`facade::edge::body`]). Each product
/// route reads the server's request id from the `X-Request-Id` header, which
/// the CDR calls carry onward.
pub fn with_middleware(product: Router, state: Arc<AppState>, server: &ServerSection) -> Router {
    let product = product.layer(axum::middleware::from_fn(correlate));
    let policy = StackPolicy {
        panic_body: panic::body(),
        refusal_body: facade::edge::body(server.body_limit_bytes, server.request_timeout()),
        request_log: RequestLog::new(state.logged_query_parameters())
            .with_recorder(state.usage_window().clone()),
    };
    stack::app(product, health(state), server, policy)
}

/// Returns the health routes' view of `state`: its indicators, its lifecycle
/// and the build facts with the pins and the lanes for `/health/info`.
fn health(state: Arc<AppState>) -> Health {
    Health::new(state.health().clone(), state.lifecycle().clone()).with_info(
        build_info::BUILD,
        move || match serde_json::to_value(state.info()) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => serde_json::Map::new(),
        },
    )
}

/// Puts the server's request id in the request's `X-Request-Id` for the
/// product routes, so a handler and the CDR calls it makes name the request
/// as the request log does.
async fn correlate(mut request: Request, next: Next) -> Response {
    let id = request
        .extensions()
        .get::<RequestId>()
        .and_then(|id| HeaderValue::from_str(id.id()).ok());
    if let Some(id) = id {
        request
            .headers_mut()
            .insert(ferrohealth_server::request_id::HEADER, id);
    }
    next.run(request).await
}

/// `GET /`: the product and the version, as JSON.
pub(crate) async fn root() -> Response {
    axum::Json(serde_json::json!({
        "product": state::PRODUCT,
        "version": state::VERSION,
    }))
    .into_response()
}

/// Serves `app` on an already-bound listener as `server` configures it, over
/// TLS when `[server.tls]` is set, until `shutdown` completes, then finishes
/// the requests in flight within the drain bound of `server`.
///
/// `lifecycle` moves to serving when this starts and to draining on the
/// signal, so readiness answers `503` while the drain runs.
///
/// # Errors
/// Returns the error of `ferrohealth_server::serve::run`: TLS files that do
/// not read, or an I/O error of the listener.
pub async fn serve_until<F>(
    listener: TcpListener,
    app: Router,
    server: &ServerSection,
    lifecycle: ferrohealth_health::lifecycle::Lifecycle,
    shutdown: F,
) -> Result<Drained, ServeError>
where
    F: Future<Output = ()> + Send + 'static,
{
    let drained =
        ferrohealth_server::serve::run(listener, app, server, lifecycle, shutdown).await?;
    if drained == Drained::Cut {
        tracing::warn!(
            drain_ms = server.shutdown_timeout().as_millis(),
            "the drain did not finish in time; the remaining connections are dropped"
        );
    }
    Ok(drained)
}
