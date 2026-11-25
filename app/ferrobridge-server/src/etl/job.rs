// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `etl run` and `etl fhir` jobs over a resolved configuration.
//!
//! The job reads the OMOCL set from `[mappings] omocl` and the running
//! revision of the console's files, builds the CDR client of `[cdr]`, the
//! concept resolver and the writer over `[cdm]`, then runs once. The binary calls [`run`] and prints the report; a test calls it with a
//! configuration of its own, so both drive the same path. No specification
//! governs the job: our own design.

use crate::config::Settings;
use crate::etl::mapper::{CdmVocabulary, MapperError, OmoclMapper, read_tree};
use crate::etl::report::RunReport;
use crate::etl::{RunError, RunOptions};
use crate::mappings::files::{Language, Root};
use crate::mappings::revisions::{Console, Revisions};
use ferrobridge_operator_api::etl::RunKind;
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
    /// The state store the console's files are read from did not open.
    #[error("opening the state store the console's mapping files are read from")]
    State(#[source] crate::store::StoreError),
    /// The running revision of the console's files could not be read.
    #[error("reading the running revision of the console's mapping files")]
    Revision(#[source] crate::mappings::revisions::RevisionError),
    /// The CDR client could not be built.
    #[error("building the CDR client the run reads through")]
    Client(#[source] crate::cdr::error::CdrError),
    /// The concept resolver's connection to the CDM database could not be
    /// prepared.
    #[error("preparing the concept resolver's connection to the CDM database")]
    Connection(#[source] omop_cdm::connection::ConnectionError),
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
    /// The rows the bridge does not own could not be counted.
    #[error("counting the CDM rows the bridge does not own")]
    Foreign(#[source] WriteError),
    /// The quality checks could not be planned.
    #[error("planning the quality checks")]
    Quality(#[source] omop_cdm::quality::plan::PlanError),
    /// The configuration lacks a section `etl fhir` needs.
    #[error("`etl fhir` needs {0}")]
    MissingFhir(&'static str),
    /// The FHIRconnect mapping set does not load.
    #[error("reading the FHIRconnect mapping set")]
    FhirMappings(#[source] crate::mappings::Error),
    /// The FHIR input does not read.
    #[error("reading the FHIR input")]
    Input(#[source] crate::etl::fhir::InputError),
    /// The run finished and its report could not be kept in the run history;
    /// the report is carried so it is still shown.
    #[error("keeping the report of run {} in the run history", .report.run_id)]
    Record {
        /// The finished run's report.
        report: Box<RunReport>,
        /// Why it could not be recorded.
        #[source]
        source: crate::etl::history::RecordError,
    },
}

/// Returns the section `etl run` needs and `settings` lacks, the first
/// missing first.
#[must_use]
pub fn missing_section(settings: &Settings) -> Option<&'static str> {
    if settings.etl.is_none() {
        Some("an [etl] section")
    } else if settings
        .etl
        .as_ref()
        .is_some_and(|etl| etl.compositions.is_none())
    {
        Some("[etl] aql")
    } else if settings.cdm.is_none() {
        Some("a [cdm] section")
    } else if settings.cdr.is_none() {
        Some("a [cdr] section")
    } else if !has_omocl(settings) {
        Some("[mappings] omocl")
    } else {
        None
    }
}

/// Whether `settings` name an OMOCL set: `[mappings] omocl`, or the console's
/// files, which the operator API saves when `[state]` is configured.
#[must_use]
pub fn has_omocl(settings: &Settings) -> bool {
    settings.omocl_directory.is_some() || console_omocl(settings)
}

/// Whether the console's files join the OMOCL set.
fn console_omocl(settings: &Settings) -> bool {
    settings.operator.is_some() && settings.state.is_some()
}

/// Reads the OMOCL set `etl run` maps with: `[mappings] omocl`, then the
/// running revision's console files, read through `store` or a store opened
/// on `[state]`.
///
/// The provisioned directory owns a path both hold, as it does in the
/// operator reload (no specification governs this: our own design).
///
/// # Errors
///
/// Returns [`JobError::Mappings`] when a directory cannot be read or the set
/// does not load, [`JobError::State`] when the store does not open and
/// [`JobError::Revision`] when the running revision cannot be read.
pub async fn omocl_set(
    settings: &Settings,
    store: Option<&crate::store::Store>,
) -> Result<omocl::model::load::MappingSet, JobError> {
    let roots: Vec<Root> = settings
        .omocl_directory
        .iter()
        .map(|directory| Root::new(Language::Omocl, directory.clone()))
        .collect();
    let console = match (console_omocl(settings), store, settings.state.as_deref()) {
        (false, _, _) | (true, None, None) => Console::default(),
        (true, Some(store), _) => Revisions::new(store)
            .console()
            .await
            .map_err(JobError::Revision)?,
        (true, None, Some(state)) => {
            let store = Box::pin(crate::store::Store::open(state))
                .await
                .map_err(JobError::State)?;
            Revisions::new(&store)
                .console()
                .await
                .map_err(JobError::Revision)?
        }
    };
    let sources = crate::mappings::files::sources(&roots, console.files(), None).map_err(
        |crate::walk::Error { path, source }| {
            JobError::Mappings(MapperError::Read { path, source })
        },
    )?;
    read_tree(&sources.omocl, settings.omocl_directory.as_deref()).map_err(JobError::Mappings)
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
    let (Some(etl), Some(cdm), Some(cdr), true) = (
        settings.etl.as_ref(),
        settings.cdm.as_ref(),
        settings.cdr.as_ref(),
        has_omocl(settings),
    ) else {
        return Err(missing());
    };
    let set = omocl_set(settings, None).await?;
    let client = crate::cdr::CdrClient::new(cdr).map_err(JobError::Client)?;
    let (mapper, pool) = mapper(etl, cdm, set).await?;
    let mut writer = CdmWriter::connect_with(
        &cdm.connection,
        cdm.schema.clone(),
        cdm.bridge_schema.clone(),
        cdm.person_policy,
    )
    .await
    .map_err(JobError::Writer)?;
    if let Some(ref pseudonymiser) = cdm.pseudonymiser {
        writer = writer.pseudonymise(pseudonymiser.clone());
    }
    let run_id = RunId::new(uuid::Uuid::new_v4().to_string()).map_err(JobError::RunId)?;
    let started = jiff::Timestamp::now();
    let mut report = crate::etl::run(etl, options, &client, &mut writer, &mapper, &run_id)
        .await
        .map_err(JobError::Run)?;
    foreign_step(&writer, &mut report).await?;
    quality_step(etl, &pool, &mut report).await?;
    keep(&writer, report, RunKind::Compositions, started).await
}

/// Records the finished run `report`, of `kind` and started at `started`,
/// in the run history of the bridge schema `writer` writes, and returns it.
///
/// # Errors
///
/// Returns [`JobError::Record`], carrying the report, when the record is
/// refused.
async fn keep(
    writer: &CdmWriter,
    report: RunReport,
    kind: RunKind,
    started: jiff::Timestamp,
) -> Result<RunReport, JobError> {
    let run = crate::etl::history::document(&report, kind, started, jiff::Timestamp::now());
    match crate::etl::history::record(writer, &run).await {
        Ok(()) => Ok(report),
        Err(source) => Err(JobError::Record {
            report: Box::new(report),
            source,
        }),
    }
}

/// Returns the OMOCL mapper over `set`, its concept resolver connected to
/// the `[cdm]` database over the TLS the configuration settled, and the
/// resolver's pool.
///
/// # Errors
///
/// Returns [`JobError::Connection`] and [`JobError::Resolver`] when the
/// resolver's connection cannot be prepared or opened.
pub(crate) async fn mapper(
    etl: &crate::config::EtlSettings,
    cdm: &crate::config::CdmSettings,
    set: omocl::model::load::MappingSet,
) -> Result<(OmoclMapper<CdmVocabulary>, CdmPool), JobError> {
    let pool = CdmPool::connect(
        sqlx::postgres::PgPoolOptions::new().max_connections(2),
        cdm.connection
            .pool_options()
            .map_err(JobError::Connection)?,
        cdm.schema.clone(),
    )
    .await
    .map_err(JobError::Resolver)?;
    let mapper = OmoclMapper::new(
        set,
        CdmVocabulary::new(ConceptResolver::new(pool.clone())),
        etl.aliases.clone(),
    );
    Ok((mapper, pool))
}

/// Counts the rows of the CDM tables the writer assigns the ids of that no
/// side-table entry owns, adds them to `report` and warns once per table.
async fn foreign_step(writer: &CdmWriter, report: &mut RunReport) -> Result<(), JobError> {
    for foreign in writer.foreign_rows().await.map_err(JobError::Foreign)? {
        // NOTE: no specification governs this: our own design; the bridge assumes it owns
        // the id space of the tables it writes, so another load's rows are a warning.
        tracing::warn!(
            table = foreign.table,
            rows = foreign.rows,
            "a CDM table holds rows the bridge did not write"
        );
        report
            .foreign_rows
            .insert(foreign.table.to_owned(), foreign.rows);
    }
    Ok(())
}

/// Runs the quality checks over the tables `report` names when `etl` asks
/// for them, and adds their verdicts to it.
async fn quality_step(
    etl: &crate::config::EtlSettings,
    pool: &CdmPool,
    report: &mut RunReport,
) -> Result<(), JobError> {
    if etl.quality_checks {
        let written = report.rows.keys().cloned().collect();
        let plan = crate::quality::plan(Some(&written)).map_err(JobError::Quality)?;
        // NOTE: no specification governs this: our own design; a failed check never rolls
        // the committed load back, it is reported.
        report.quality = Some(crate::quality::run(pool, &plan).await.verdicts());
    }
    Ok(())
}

/// Returns the section `etl fhir` needs and `settings` lacks, the first
/// missing first.
#[must_use]
pub fn missing_fhir_section(settings: &Settings) -> Option<&'static str> {
    if settings.etl.is_none() {
        Some("an [etl] section")
    } else if settings.cdm.is_none() {
        Some("a [cdm] section")
    } else if settings.mappings.is_none() {
        Some("[mappings] directory and templates")
    } else if !has_omocl(settings) {
        Some("[mappings] omocl")
    } else {
        None
    }
}

/// Runs `etl fhir` once over the FHIR resources at `input` and returns its
/// report.
///
/// The FHIRconnect set loads from `[mappings] directory` against the
/// operational templates of `[mappings] templates`, with no CDR, and the
/// OMOCL set as `etl run` reads it; both load and the input reads before the
/// database is reached.
///
/// # Errors
///
/// Returns [`JobError::MissingFhir`] naming the first section the
/// configuration lacks, [`JobError::FhirMappings`] and [`JobError::Mappings`]
/// when a mapping set does not load, [`JobError::Input`] when the input does
/// not read, the connection variants when a client cannot connect, and
/// [`JobError::Run`] when the run stops.
pub async fn run_fhir(settings: &Settings, input: &std::path::Path) -> Result<RunReport, JobError> {
    let missing =
        || JobError::MissingFhir(missing_fhir_section(settings).unwrap_or("its configuration"));
    let (Some(etl), Some(cdm), Some(mappings), true) = (
        settings.etl.as_ref(),
        settings.cdm.as_ref(),
        settings.mappings.as_ref(),
        has_omocl(settings),
    ) else {
        return Err(missing());
    };
    let programs = crate::mappings::load(mappings).map_err(JobError::FhirMappings)?;
    let set = omocl_set(settings, None).await?;
    let resources = crate::etl::fhir::read_input(input).map_err(JobError::Input)?;
    let (mapper, pool) = mapper(etl, cdm, set).await?;
    let mut writer = CdmWriter::connect_with(
        &cdm.connection,
        cdm.schema.clone(),
        cdm.bridge_schema.clone(),
        cdm.person_policy,
    )
    .await
    .map_err(JobError::Writer)?;
    if let Some(ref pseudonymiser) = cdm.pseudonymiser {
        writer = writer.pseudonymise(pseudonymiser.clone());
    }
    let run_id = RunId::new(uuid::Uuid::new_v4().to_string()).map_err(JobError::RunId)?;
    let started = jiff::Timestamp::now();
    let operations = &settings.operations;
    let now = jiff::Timestamp::now().to_string();
    let side = crate::etl::fhir::FhirSide {
        programs: &programs,
        device: &operations.device_reference,
        composer: &operations.composer,
        language: operations.composition_language.as_deref(),
        territory: operations.composition_territory.as_deref(),
        now: &now,
    };
    let mut report = crate::etl::fhir::run(&resources, &side, etl, &mut writer, &mapper, &run_id)
        .await
        .map_err(JobError::Run)?;
    foreign_step(&writer, &mut report).await?;
    quality_step(etl, &pool, &mut report).await?;
    keep(&writer, report, RunKind::Fhir, started).await
}
