// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The batch jobs beside `serve`: `cdm init`, `etl run` and `mapping check`.
//!
//! Each builds its own runtime, does its one job and prints what it did for
//! the operator. No specification governs the job output: our own design.

use crate::config;
use crate::config::Settings;
use crate::etl;

/// Applies the CDM DDL and the bridge schema to the configured database, and
/// prints what it found and did.
///
/// The CDM schema is built in one transaction through `sqlx`, and the bridge
/// schema through the writer's own `tokio-postgres` connection; the two
/// clients never share a pool, and both connect over the TLS the `[cdm]`
/// section settles. A schema that already holds every CDM table is reported
/// and left as it is; one that holds some of them is refused. With
/// `with_constraints`, OHDSI's foreign keys follow in a transaction of their
/// own, so a refusal there leaves the tables in place and names the refused
/// statement.
#[expect(
    clippy::print_stdout,
    reason = "what the job found and did is its output, written to stdout for the operator"
)]
pub(crate) fn cdm_init_command(
    cdm: &config::CdmSettings,
    with_constraints: bool,
) -> anyhow::Result<()> {
    use anyhow::Context;
    use omop_cdm::database::{self, CONSTRAINTS_FILE, Init};

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        tracing::info!(
            schema = %cdm.schema,
            ssl_mode = %cdm.connection.ssl_mode(),
            ca = cdm.connection.has_ca(),
            "connecting to the CDM database"
        );
        let pool = database::CdmPool::connect(
            sqlx::postgres::PgPoolOptions::new().max_connections(1),
            cdm.connection.pool_options(),
            cdm.schema.clone(),
        )
        .await
        .context("connecting to the CDM database")?;
        let outcome = database::init(&pool)
            .await
            .with_context(|| format!("applying the CDM DDL to schema {}", cdm.schema))?;
        match &outcome {
            Init::Created => println!(
                "CDM schema {}: created the CDM {} tables, primary keys and indices (OHDSI {})",
                cdm.schema,
                omop_cdm::CDM_VERSION,
                omop_cdm::CDM_DEFINITIONS_TAG
            ),
            Init::AlreadyInitialised { cdm_versions } if cdm_versions.is_empty() => println!(
                "CDM schema {}: already initialised, every CDM {} table is present and \
                 CDM_SOURCE holds no row; nothing applied",
                cdm.schema,
                omop_cdm::CDM_VERSION
            ),
            Init::AlreadyInitialised { cdm_versions } => println!(
                "CDM schema {}: already initialised, CDM_SOURCE records cdm_version {}; \
                 nothing applied",
                cdm.schema,
                cdm_versions.join(", ")
            ),
        }
        if with_constraints {
            let applied = database::constrain(&pool)
                .await
                .with_context(|| format!("applying {CONSTRAINTS_FILE} to schema {}", cdm.schema))?;
            println!(
                "CDM schema {}: applied the {applied} foreign keys of {CONSTRAINTS_FILE}",
                cdm.schema
            );
        }
        pool.pool().close().await;
        let mut writer = omop_cdm::writer::CdmWriter::connect_with(
            &cdm.connection,
            cdm.schema.clone(),
            cdm.bridge_schema.clone(),
            cdm.person_policy,
        )
        .await
        .context("connecting the CDM writer")?;
        writer
            .init()
            .await
            .with_context(|| format!("creating the bridge schema {}", cdm.bridge_schema))?;
        println!("bridge schema {}: in place", cdm.bridge_schema);
        tracing::info!(
            schema = %cdm.schema,
            bridge_schema = %cdm.bridge_schema,
            "the CDM schema and the bridge schema are in place"
        );
        Ok(())
    })
}

/// Runs the OMOP ETL once and prints its report as text and as JSON.
///
/// The job itself is [`etl::job::run`], which a test drives with a
/// configuration of its own.
#[expect(
    clippy::print_stdout,
    reason = "the run report is the job's output, written to stdout for the operator"
)]
pub(crate) fn etl_run_command(
    settings: &Settings,
    options: &etl::RunOptions,
) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let report = runtime.block_on(etl::job::run(settings, options))?;
    println!("{report}");
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Why `mapping check` stopped before it printed a report.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CheckFailure {
    /// The configuration names nothing the check can run over.
    #[error(transparent)]
    Configuration(crate::check::Error),
    /// The check could not run.
    #[error(transparent)]
    Run(anyhow::Error),
}

/// Checks the configured mapping set and prints the report in `format`.
///
/// Returns whether the set is clean. The report goes to stdout and nothing
/// else does, so the JSON form reads as one document.
#[expect(
    clippy::print_stdout,
    reason = "the check report is the job's output, written to stdout for the operator"
)]
pub(crate) fn mapping_check_command(
    settings: &Settings,
    format: crate::cli::ReportFormat,
) -> Result<bool, CheckFailure> {
    use anyhow::Context;

    let needs_cdr = settings.mapping_directory.is_some() && settings.mappings.is_none();
    let cdr = match settings.cdr.as_ref() {
        Some(config) if needs_cdr => Some(
            crate::cdr::CdrClient::new(config)
                .context("building the CDR client the templates are fetched through")
                .map_err(CheckFailure::Run)?,
        ),
        _ => None,
    };
    let inputs =
        crate::check::inputs(settings, cdr.as_ref()).map_err(CheckFailure::Configuration)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building the runtime")
        .map_err(CheckFailure::Run)?;
    let report = runtime
        .block_on(crate::check::check(inputs))
        .context("checking the mapping set")
        .map_err(CheckFailure::Run)?;
    match format {
        crate::cli::ReportFormat::Text => println!("{report}"),
        crate::cli::ReportFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&report.to_json())
                .context("rendering the report")
                .map_err(CheckFailure::Run)?
        ),
    }
    Ok(report.is_clean())
}
