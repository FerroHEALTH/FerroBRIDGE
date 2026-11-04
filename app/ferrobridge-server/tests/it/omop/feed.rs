// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Both change-feed sources over the reference CDR, end to end, behind the
//! `FERROBRIDGE_E2E` gate: the round trip's three laboratory compositions are
//! committed over ITS-REST 1.1.0, and the incremental OMOP load follows them
//! from the source itself, a page read after the reader's cursor, each event
//! processed, the pass settled and the page acknowledged. Over the reference
//! CDR's outbox feed a `composition_delete` (openEHR RM Common 1.1.0 §6.2.5)
//! then reaches the load as a `523` event and retracts the composition's
//! rows. Over the AQL source the same commits reach the load from
//! `VERSION v[ALL_VERSIONS]` past the watermark. No specification governs the
//! change feed: our own design.

use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::change_feed::follow::{Consumer, Polling};
use ferrobridge_server::change_feed::{ChangeFeed, Event, PageSize, ReaderName, Settings, Source};
use ferrobridge_server::config::Process;
use ferrobridge_server::etl::incremental::Incremental;
use ferrobridge_testkit::containers;
use ferrobridge_testkit::fixtures::LABORATORY_REPORT_OPT;
use openehr_its::rest::generated::ehr::client::CompositionDeleteOutcome;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use super::support::{
    LABORATORIES, build_cdm, canonical, configuration, copy_mappings, create, create_ehr, rows,
    upload_template,
};

/// The passes a drain reads at most before it gives up waiting.
const PASSES: usize = 40;

/// Returns the number of `cdm.measurement` rows.
async fn measurements(postgres: &containers::Postgres) -> Result<String, Box<dyn Error>> {
    let answered = rows(postgres, "SELECT count(*) FROM cdm.measurement").await?;
    Ok(answered
        .first()
        .and_then(|row| row.first())
        .ok_or("one cell")?
        .clone())
}

/// Follows `feed` under `reader` with `load` until events naming `wanted`
/// composition versions have been processed, settling and acknowledging each page as the follower does, and
/// returns the events.
async fn drain(
    feed: &dyn ChangeFeed,
    reader: &ReaderName,
    load: &Incremental,
    wanted: usize,
) -> Result<Vec<Event>, Box<dyn Error>> {
    let mut seen = Vec::new();
    for _ in 0..PASSES {
        let page = feed
            .page(reader, PageSize::new(100).ok_or("a page size")?)
            .await?;
        for event in &page.events {
            load.process(event).await?;
        }
        load.settle().await?;
        if let Some(last) = page.events.last() {
            feed.acknowledge(reader, &last.position).await?;
        }
        seen.extend(page.events);
        if changed(&seen) >= wanted {
            return Ok(seen);
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    Err(format!(
        "the feed offered {} of {wanted} composition versions",
        changed(&seen)
    )
    .into())
}

/// Returns how many composition versions `events` name.
fn changed(events: &[Event]) -> usize {
    events
        .iter()
        .flat_map(|event| &event.versions)
        .filter(|version| version.kind == "COMPOSITION")
        .count()
}

/// One running case: the CDR, the CDM database, the committed compositions
/// and the incremental load.
struct Case {
    /// The reference CDR.
    cdr: containers::Cdr,
    /// The CDM database.
    postgres: containers::Postgres,
    /// The CDR client.
    client: CdrClient,
    /// Every committed composition, with its EHR.
    committed: Vec<(
        ferrobridge_server::cdr::ids::EhrId,
        openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId,
    )>,
    /// The load.
    load: Incremental,
    /// The scratch directories, kept for the case.
    _scratch: (tempfile::TempDir, tempfile::TempDir),
}

/// Starts the CDR and the CDM, commits the three compositions and starts
/// the load.
async fn case() -> Result<Case, Box<dyn Error>> {
    let cdr = containers::cdr().await?;
    let postgres = containers::postgres().await?;
    build_cdm(&postgres).await?;
    upload_template(&cdr).await?;
    let client = CdrClient::new(&CdrConfig::new(format!("{}/", cdr.base_url()).parse()?))?;
    let ehrs = [create_ehr(&client).await?, create_ehr(&client).await?];
    let index = WebTemplateIndex::build(&TemplateSource::opt14(LABORATORY_REPORT_OPT)?)?;
    let mut committed = Vec::new();
    for laboratory in &LABORATORIES {
        let ehr = ehrs.get(laboratory.ehr).ok_or("an EHR")?;
        let version = create(&client, ehr, &canonical(&index, laboratory)?).await?;
        committed.push((ehr.clone(), version));
    }
    let scratch = tempfile::tempdir()?;
    let mappings = tempfile::tempdir()?;
    copy_mappings(mappings.path())?;
    let config = configuration(scratch.path(), &cdr, &postgres, mappings.path())?;
    let settings = ferrobridge_server::config::Config::from_loaded(
        ferrobridge_server::config::Config::load(Some(&config), &[])?,
    )
    .resolve(Process::Server)?;
    let load = Incremental::start(&settings, client.clone()).await?;
    Ok(Case {
        cdr,
        postgres,
        client,
        committed,
        load,
        _scratch: (scratch, mappings),
    })
}

/// Returns the change feed over `source`, its dead letters in `directory`.
fn feed(
    source: Source,
    directory: &std::path::Path,
    client: CdrClient,
) -> Result<Arc<dyn ChangeFeed>, Box<dyn Error>> {
    let settings = Settings {
        source,
        polling: Polling::default(),
        dead_letters: directory.join("dead-letters.redb"),
        max_attempts: NonZeroU32::MIN,
    };
    Ok(ferrobridge_server::change_feed::open(&settings, client)?)
}

#[tokio::test]
async fn the_outbox_feed_drives_the_incremental_load_through_a_deletion()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let case = case().await?;
    let scratch = tempfile::tempdir()?;
    let feed = feed(
        Source::FerroEhr {
            root: case.cdr.extensions_url().parse()?,
        },
        scratch.path(),
        case.client.clone(),
    )?;
    let reader = ReaderName::new(containers::CDR_READERS[0])?;
    let committed = drain(feed.as_ref(), &reader, &case.load, 3).await?;
    assert_eq!(
        3,
        changed(&committed),
        "one version per committed composition"
    );
    assert_eq!(
        "6",
        measurements(&case.postgres).await?,
        "every commit loads"
    );
    let (doomed_ehr, doomed) = case.committed.get(1).ok_or("the second composition")?;
    match case
        .client
        .delete_composition(doomed_ehr, doomed)
        .await?
        .outcome
    {
        CompositionDeleteOutcome::NoContent { .. } => {}
        other => return Err(format!("the CDR refused the delete: {other:?}").into()),
    }
    let deleted = drain(feed.as_ref(), &reader, &case.load, 1).await?;
    assert!(
        deleted
            .iter()
            .flat_map(|event| &event.versions)
            .any(|version| version.change_type == "523"),
        "the feed serves the deletion as a 523: {deleted:?}"
    );
    assert_eq!(
        "4",
        measurements(&case.postgres).await?,
        "the deletion retracts its rows"
    );
    Ok(())
}

#[tokio::test]
async fn the_aql_source_drives_the_incremental_load_over_the_same_commits()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let case = case().await?;
    let scratch = tempfile::tempdir()?;
    let feed = feed(
        Source::Aql {
            watermarks: scratch.path().join("watermarks.redb"),
            start: ferrobridge_server::change_feed::aql::Start::Beginning,
        },
        scratch.path(),
        case.client.clone(),
    )?;
    let reader = ReaderName::new("ferrobridge-aql")?;
    let committed = drain(feed.as_ref(), &reader, &case.load, 3).await?;
    assert_eq!(3, changed(&committed), "one event per committed version");
    assert_eq!(
        "6",
        measurements(&case.postgres).await?,
        "every commit loads"
    );
    let again = feed
        .page(&reader, PageSize::new(100).ok_or("a page size")?)
        .await?;
    assert_eq!(
        0,
        again.events.len(),
        "the watermark holds past every finished event"
    );
    Ok(())
}
