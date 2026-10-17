// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `etl run` job over a resolved configuration.
//!
//! The job reads the OMOCL set from `[mappings] omocl` and the console's
//! `[operator] mapping_dir`, builds the CDR client of `[cdr]`, the concept
//! resolver and the writer over `[cdm]`, then runs once. The binary calls [`run`] and prints the report; a test calls it with a
//! configuration of its own, so both drive the same path. No specification
//! governs the job: our own design.

use crate::config::Settings;
use crate::etl::mapper::{CdmVocabulary, MapperError, OmoclMapper, read_roots};
use crate::etl::report::RunReport;
use crate::etl::{RunError, RunOptions};
use crate::operator::files::{Language, Root};
use omop_cdm::database::{CdmPool, ConnectError};
use omop_cdm::graph::key::EmptyIdentifier;
use omop_cdm::vocabulary::ConceptResolver;
use omop_cdm::writer::{CdmWriter, error::WriteError, input::RunId};

/// Why `etl run` could not run.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum JobError {
    /// The configuration lacks a section the job needs.
    #[error("`etl run` needs {0}")]
    Missing(&'static str),
    /// The OMOCL mapping set does not load.
    #[error("reading the OMOCL mapping set")]
    Mappings(#[source] MapperError),
    /// The CDR client could not be built.
    #[error("building the CDR client the run reads through")]
    Client(#[source] crate::cdr::error::CdrError),
    /// The concept resolver could not connect to the CDM database.
    #[error("connecting the concept resolver to the CDM database")]
    Resolver(#[source] ConnectError),
    /// The writer could not connect to the CDM database.
    #[error("connecting the CDM writer")]
    Writer(#[source] WriteError),
    /// The run identifier could not be formed.
    #[error("forming the run identifier")]
    RunId(#[source] EmptyIdentifier),
    /// The run stopped.
    #[error("running the ETL")]
    Run(#[source] RunError),
}

/// Returns the section `etl run` needs and `settings` lacks, the first
/// missing first.
#[must_use]
pub fn missing_section(settings: &Settings) -> Option<&'static str> {
    if settings.etl.is_none() {
        Some("an [etl] section")
    } else if settings.cdm.is_none() {
        Some("a [cdm] section")
    } else if settings.cdr.is_none() {
        Some("a [cdr] section")
    } else if omocl_roots(settings).is_empty() {
        Some("[mappings] omocl")
    } else {
        None
    }
}

/// Returns the OMOCL directories `etl run` reads: `[mappings] omocl`, then
/// the console's `omocl/` under `[operator] mapping_dir` when the operator
/// API is on.
///
/// The provisioned directory comes first, so it owns a path both hold, as it
/// does in the operator reload (no specification governs this: our own
/// design).
#[must_use]
pub fn omocl_roots(settings: &Settings) -> Vec<Root> {
    let provisioned = settings
        .omocl_directory
        .as_ref()
        .map(|directory| Root::new(Language::Omocl, directory.clone()));
    let console = settings
        .operator
        .as_ref()
        .and_then(|operator| operator.mapping_dir.as_deref())
        .map(|mapping_dir| {
            Root::console(
                Language::Omocl,
                Language::Omocl.console_directory(mapping_dir),
            )
        });
    provisioned.into_iter().chain(console).collect()
}

/// Runs the OMOP ETL once over `settings` and returns its report.
///
/// The OMOCL set is read before any upstream is called, so a mapping that
/// does not load refuses the start. The concept resolver and the writer open
/// their own connections to the `[cdm]` database, over the TLS the
/// configuration settled; the two clients never share a pool. The run carries
/// a fresh UUID as its identifier.
///
/// # Errors
///
/// Returns [`JobError::Missing`] naming the first section the configuration
/// lacks, [`JobError::Mappings`] when the OMOCL set does not load, the
/// connection variants when a client cannot be built or connected, and
/// [`JobError::Run`] when the run stops.
pub async fn run(settings: &Settings, options: &RunOptions) -> Result<RunReport, JobError> {
    let missing = || JobError::Missing(missing_section(settings).unwrap_or("its configuration"));
    let roots = omocl_roots(settings);
    let (Some(etl), Some(cdm), Some(cdr), false) = (
        settings.etl.as_ref(),
        settings.cdm.as_ref(),
        settings.cdr.as_ref(),
        roots.is_empty(),
    ) else {
        return Err(missing());
    };
    let set = read_roots(&roots).map_err(JobError::Mappings)?;
    let client = crate::cdr::CdrClient::new(cdr).map_err(JobError::Client)?;
    let pool = CdmPool::connect(
        sqlx::postgres::PgPoolOptions::new().max_connections(2),
        cdm.connection.pool_options(),
        cdm.schema.clone(),
    )
    .await
    .map_err(JobError::Resolver)?;
    let mapper = OmoclMapper::new(
        set,
        CdmVocabulary::new(ConceptResolver::new(pool)),
        etl.aliases.clone(),
    );
    let mut writer = CdmWriter::connect_with(
        &cdm.connection,
        cdm.schema.clone(),
        cdm.bridge_schema.clone(),
        cdm.person_policy,
    )
    .await
    .map_err(JobError::Writer)?;
    let run_id = RunId::new(uuid::Uuid::new_v4().to_string()).map_err(JobError::RunId)?;
    crate::etl::run(etl, options, &client, &mut writer, &mapper, &run_id)
        .await
        .map_err(JobError::Run)
}
