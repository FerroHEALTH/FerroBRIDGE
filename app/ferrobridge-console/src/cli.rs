// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The command line: the family's `serve`, `config check` and `healthcheck`
//! from `ferrohealth-server`, with its `--config` and `--set`.
//!
//! `serve` runs the console, `config check` reads and resolves the
//! configuration the way `serve` would and exits, and `healthcheck` asks the
//! console on this host whether it is ready, for a container runtime with no
//! HTTP client of its own. No specification governs the command line: our own
//! design.

use clap::Parser;
use ferrohealth_server::cli::{Common, ConfigArgs};

/// The `ferrobridge-console` command line.
#[derive(Debug, Parser, PartialEq, Eq)]
#[command(
    name = "ferrobridge-console",
    version,
    about = "The FerroBRIDGE Console"
)]
pub struct Cli {
    /// The configuration file to read, overriding `FERROBRIDGE_CONSOLE_CONFIG`,
    /// and the `--set key=value` pairs applied over it and the environment.
    #[command(flatten)]
    pub config: ConfigArgs,
    /// The job to run.
    #[command(subcommand)]
    pub command: Common,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::{CommandFactory, Parser};
    use ferrohealth_server::cli::{Common, ConfigCommand};

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_job_parses_with_and_without_a_configuration_file() {
        for (args, command) in [
            (vec!["ferrobridge-console", "serve"], Common::Serve),
            (
                vec!["ferrobridge-console", "config", "check"],
                Common::Config {
                    command: ConfigCommand::Check,
                },
            ),
            (
                vec!["ferrobridge-console", "healthcheck"],
                Common::Healthcheck,
            ),
        ] {
            let parsed = Cli::try_parse_from(&args).expect("the job parses");
            assert_eq!(command, parsed.command, "{args:?}");
            assert_eq!(None, parsed.config.config, "{args:?}");
        }
        let parsed =
            Cli::try_parse_from(["ferrobridge-console", "serve", "--config", "console.toml"])
                .expect("the job parses");
        assert_eq!(Some("console.toml".into()), parsed.config.config);
        let parsed = Cli::try_parse_from([
            "ferrobridge-console",
            "serve",
            "--set",
            "server.listen=0.0.0.0:3000",
        ])
        .expect("the job parses");
        assert_eq!(1, parsed.config.sets.len());
    }
}
