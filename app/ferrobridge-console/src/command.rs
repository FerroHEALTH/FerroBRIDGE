// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The run path of the `ferrobridge-console` binary: the command line parsed,
//! the configuration read, and each job run to its exit code.

use std::process::ExitCode;

use ferrohealth_server::cli::{Common, ConfigCommand};
use ferrohealth_server::run::{EXIT_CONFIG, Run, chain};
use ferrohealth_telemetry::telemetry::{Console, Service};

use crate::cli::Cli;
use crate::config::Config;
use crate::config::settings::Settings;
use crate::server::ConsoleState;

/// How long `healthcheck` waits for the console's answer.
pub const HEALTHCHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Runs the binary with `args` and returns the process exit code.
///
/// `args` is the whole argument vector, the program name included, so the
/// command-line parser reports usage under the right name. The family's run
/// path parses it, answers help, the version and a usage error with clap's
/// exit code, and runs the job on its multi-threaded runtime.
#[must_use]
pub fn run<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    Run::new("ferrobridge-console").run(args, |cli: Cli| async move { Ok(dispatch(cli).await) })
}

/// Runs the job `cli` names and returns the process exit code.
#[expect(
    clippy::print_stderr,
    reason = "a refused configuration is reported before any log subscriber exists"
)]
async fn dispatch(cli: Cli) -> ExitCode {
    let loaded = match Config::load(cli.config.config.as_deref(), &cli.config.sets) {
        Ok(loaded) => loaded,
        Err(error) if cli.command == Common::Healthcheck => {
            eprintln!("ferrobridge-console: not up: {}", chain(&error));
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("ferrobridge-console: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG.get());
        }
    };
    let report = (cli.command
        == Common::Config {
            command: ConfigCommand::Check,
        })
    .then(|| ferrohealth_config::report::check_report(&loaded));
    let settings = match loaded.into_parts().0.resolve() {
        Ok(settings) => settings,
        Err(error) if cli.command == Common::Healthcheck => {
            eprintln!("ferrobridge-console: not up: {}", chain(&error));
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("ferrobridge-console: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG.get());
        }
    };
    match cli.command {
        Common::Healthcheck => healthcheck(&settings).await,
        Common::Config {
            command: ConfigCommand::Check,
        } => config_check(settings, report).await,
        Common::Serve => serve(settings).await,
    }
}

/// Runs `config check`: the running tree with every secret redacted, then the
/// state the server would start with is built, and nothing is bound.
///
/// No provider is contacted: with an `[oidc]` that leaves an endpoint to
/// discovery, the bridge client alone is built.
#[expect(
    clippy::print_stdout,
    reason = "the job's one answer goes to standard output"
)]
#[expect(
    clippy::print_stderr,
    reason = "the job's refusal goes to standard error"
)]
async fn config_check(
    settings: Settings,
    report: Option<Result<String, ferrohealth_config::error::ConfigError>>,
) -> ExitCode {
    match report {
        Some(Ok(report)) => println!("{report}"),
        Some(Err(error)) => {
            eprintln!("ferrobridge-console: cannot start: {}", chain(&error));
            return ExitCode::from(EXIT_CONFIG.get());
        }
        None => {}
    }
    let offline = settings
        .oidc
        .as_ref()
        .is_none_or(crate::config::settings::OidcSettings::endpoints_given);
    let built = if offline {
        ConsoleState::new(settings).await.map(drop)
    } else {
        crate::bridge::Bridge::new(&settings.bridge)
            .map(drop)
            .map_err(crate::server::StateError::from)
    };
    match built {
        Ok(()) => {
            println!("ferrobridge-console: the configuration is valid");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ferrobridge-console: cannot start: {}", chain(&error));
            ExitCode::from(EXIT_CONFIG.get())
        }
    }
}

/// Runs `healthcheck` against the console this configuration describes.
#[expect(
    clippy::print_stdout,
    reason = "the job's one-line answer goes to standard output"
)]
#[expect(
    clippy::print_stderr,
    reason = "the job's one-line refusal goes to standard error"
)]
async fn healthcheck(settings: &Settings) -> ExitCode {
    let target = ferrohealth_server::healthcheck::target(settings.listen);
    let outcome = ferrohealth_server::healthcheck::check(target, "", HEALTHCHECK_TIMEOUT).await;
    if outcome.exit_code() == 0 {
        println!("ferrobridge-console: {outcome}");
        ExitCode::SUCCESS
    } else {
        eprintln!("ferrobridge-console: {outcome}");
        ExitCode::FAILURE
    }
}

/// Runs `serve`: the subscriber, the state, and the server until `SIGTERM`
/// or `SIGINT`.
#[expect(
    clippy::print_stderr,
    reason = "a refusal before the subscriber starts has no other place to go"
)]
async fn serve(settings: Settings) -> ExitCode {
    let service = Service::new("ferrobridge-console", env!("CARGO_PKG_VERSION"));
    let telemetry = match ferrohealth_server::telemetry::build(&settings.telemetry, service) {
        Ok(telemetry) => telemetry,
        Err(error) => {
            eprintln!(
                "ferrobridge-console: cannot start the log: {}",
                chain(&error)
            );
            return ExitCode::FAILURE;
        }
    };
    let telemetry = match telemetry
        .install(&settings.telemetry, &Console::detect())
        .map(|_filter| telemetry)
    {
        Ok(telemetry) => telemetry,
        Err(error) => {
            eprintln!(
                "ferrobridge-console: cannot start the log: {}",
                chain(&error)
            );
            return ExitCode::FAILURE;
        }
    };
    ferrohealth_telemetry::panic::install_hook();
    let listen = settings.listen;
    let state = match ConsoleState::new(settings).await {
        Ok(state) => state,
        Err(error) => {
            tracing::error!(error = %chain(&error), "cannot start");
            return ExitCode::from(EXIT_CONFIG.get());
        }
    };
    let listener = match tokio::net::TcpListener::bind(listen).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%listen, %error, "cannot bind the listen address");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!(%listen, "the operator console is listening");
    let lifecycle = state.lifecycle().clone();
    let served = crate::server::serve(
        listener,
        crate::server::router(state),
        lifecycle,
        ferrohealth_server::serve::shutdown_signal(),
    );
    let code = match served.await {
        Ok(_drained) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "the listener failed");
            ExitCode::FAILURE
        }
    };
    if let Err(error) = telemetry.shutdown() {
        tracing::warn!(%error, "the telemetry did not flush");
    }
    code
}
