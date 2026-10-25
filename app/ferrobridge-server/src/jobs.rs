// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The batch jobs beside `serve`: `cdm init`, `vocab load`, `etl run` and
//! `mapping check`.
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
            cdm.connection
                .pool_options()
                .context("preparing the connection to the CDM database")?,
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
        for foreign in writer
            .foreign_rows()
            .await
            .context("counting the CDM rows the bridge does not own")?
        {
            println!(
                "warning: {} holds {} rows the bridge did not write; one bridge owns the tables \
                 it writes",
                foreign.table, foreign.rows
            );
        }
        tracing::info!(
            schema = %cdm.schema,
            bridge_schema = %cdm.bridge_schema,
            "the CDM schema and the bridge schema are in place"
        );
        Ok(())
    })
}

/// Loads the Athena vocabulary download in `directory` into the vocabulary
/// tables of `schema` in the configured database, and prints what it wrote.
///
/// Each table is replaced in a transaction of its own
/// ([`omop_cdm::athena::load`]); the files the download carries beside the
/// vocabulary tables are named and left alone.
#[expect(
    clippy::print_stdout,
    reason = "what the job loaded is its output, written to stdout for the operator"
)]
pub(crate) fn vocab_load_command(
    cdm: &config::CdmSettings,
    directory: &std::path::Path,
    schema: &str,
) -> anyhow::Result<()> {
    use anyhow::Context;
    use omop_cdm::athena::{self, Export};

    let schema = omop_cdm::ddl::SchemaName::new(schema)
        .with_context(|| format!("the schema name {schema}"))?;
    let export = Export::read(directory)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        tracing::info!(
            schema = %schema,
            directory = %directory.display(),
            ssl_mode = %cdm.connection.ssl_mode(),
            "connecting to the CDM database"
        );
        let pool = omop_cdm::database::CdmPool::connect(
            sqlx::postgres::PgPoolOptions::new().max_connections(1),
            cdm.connection
                .pool_options()
                .context("preparing the connection to the CDM database")?,
            schema.clone(),
        )
        .await
        .context("connecting to the CDM database")?;
        let loaded = athena::load(&pool, &export)
            .await
            .with_context(|| format!("loading the vocabulary into schema {schema}"))?;
        pool.pool().close().await;
        for (table, rows) in loaded.tables() {
            println!("vocabulary schema {schema}: {table} holds {rows} rows");
        }
        for name in export.ignored() {
            println!("vocabulary schema {schema}: {name} is not a vocabulary table, left alone");
        }
        match loaded.vocabulary_version() {
            Some(version) => println!("vocabulary schema {schema}: vocabulary version {version}"),
            None => println!(
                "vocabulary schema {schema}: VOCABULARY holds no `None` row, so the download names no version"
            ),
        }
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

/// Runs every ported Data Quality Dashboard check against the configured
/// database and prints the result in `format`; returns whether every check
/// passed or did not apply.
#[expect(
    clippy::print_stdout,
    reason = "the check result is the job's output, written to stdout for the operator"
)]
pub(crate) fn dq_check_command(
    cdm: &config::CdmSettings,
    format: crate::cli::ReportFormat,
) -> anyhow::Result<bool> {
    use anyhow::Context;

    let plan = crate::quality::plan(None).context("planning the checks")?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let pool = omop_cdm::database::CdmPool::connect(
            sqlx::postgres::PgPoolOptions::new().max_connections(1),
            cdm.connection
                .pool_options()
                .context("preparing the connection to the CDM database")?,
            cdm.schema.clone(),
        )
        .await
        .context("connecting to the CDM database")?;
        let run = crate::quality::run(&pool, &plan).await;
        pool.pool().close().await;
        match format {
            crate::cli::ReportFormat::Text => print!("{run}"),
            crate::cli::ReportFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&run.dashboard_json())?);
            }
        }
        Ok(run.clean())
    })
}

/// Loads the FHIR resources at `input` into the CDM once and prints the
/// report as text and as JSON.
///
/// The job itself is [`etl::job::run_fhir`].
#[expect(
    clippy::print_stdout,
    reason = "the run report is the job's output, written to stdout for the operator"
)]
pub(crate) fn etl_fhir_command(settings: &Settings, input: &std::path::Path) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let report = runtime.block_on(etl::job::run_fhir(settings, input))?;
    println!("{report}");
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Checks the OMOCL literals of `files` against the vocabulary of the `cdm`
/// database.
async fn vocabulary_diagnostics(
    cdm: &config::CdmSettings,
    files: &[std::path::PathBuf],
) -> anyhow::Result<Vec<openehr_mapping_core::diagnostic::Diagnostic>> {
    use anyhow::Context;

    let pool = omop_cdm::database::CdmPool::connect(
        sqlx::postgres::PgPoolOptions::new().max_connections(1),
        cdm.connection
            .pool_options()
            .context("preparing the connection to the CDM database")?,
        cdm.schema.clone(),
    )
    .await
    .context("connecting to the CDM database")?;
    let resolver = omop_cdm::vocabulary::ConceptResolver::new(pool.clone());
    let diagnostics = crate::check::concept_domains(files, &resolver)
        .await
        .context("looking the literal concepts up in the vocabulary")?;
    pool.pool().close().await;
    Ok(diagnostics)
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
    vocabulary: bool,
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
    let omocl = inputs.omocl.map(std::path::Path::to_path_buf);
    let mut report = runtime
        .block_on(crate::check::check(inputs))
        .context("checking the mapping set")
        .map_err(CheckFailure::Run)?;
    if vocabulary {
        let cdm = settings.cdm.as_ref().ok_or(CheckFailure::Configuration(
            crate::check::Error::NoVocabulary,
        ))?;
        let files = match omocl {
            Some(directory) => {
                crate::check::omocl_files(&directory).map_err(CheckFailure::Configuration)?
            }
            None => {
                return Err(CheckFailure::Configuration(
                    crate::check::Error::NoVocabulary,
                ));
            }
        };
        let diagnostics = runtime
            .block_on(vocabulary_diagnostics(cdm, &files))
            .map_err(CheckFailure::Run)?;
        for diagnostic in diagnostics {
            report = report.with_diagnostic(diagnostic);
        }
    }
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
