// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The incremental OMOP load over the reference CDR and a CDM v5.4
//! PostgreSQL, behind the `FERROBRIDGE_E2E` gate.
//!
//! The round trip's three laboratory compositions are committed over
//! ITS-REST 1.1.0, and the incremental load is handed one change-feed event
//! per composition, then settles the pass. A batch `etl run` after it leaves
//! the CDM database as the load wrote it. The second composition is then
//! deleted with `composition_delete` (openEHR RM Common 1.1.0 §6.2.5); its
//! event retracts the composition's rows, and a batch run after that changes
//! nothing again.

use std::error::Error;

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::change_feed::follow::Consumer;
use ferrobridge_server::change_feed::{ChangedVersion, Domain, Event, Position, Seq};
use ferrobridge_server::config::Process;
use ferrobridge_server::etl::incremental::Incremental;
use ferrobridge_testkit::containers;
use ferrobridge_testkit::fixtures::LABORATORY_REPORT_OPT;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_its::rest::generated::ehr::client::CompositionDeleteOutcome;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use super::support::{
    LABORATORIES, build_cdm, canonical, configuration, copy_mappings, create, create_ehr, etl_run,
    rows, upload_template,
};

/// Returns the event of the commit of `version` in `ehr`, at `seq`, with
/// the audit change type `change_type`.
fn event(
    seq: u64,
    ehr: &EhrId,
    version: &ObjectVersionId,
    change_type: &str,
) -> Result<Event, Box<dyn Error>> {
    Ok(Event {
        position: Position::Outbox {
            domain: Domain::Clinical,
            seq: Seq::new(seq),
        },
        erasure: false,
        contribution_id: None,
        ehr_id: Some(ehr.clone()),
        committed_at: None,
        versions: vec![ChangedVersion {
            vo_id: Some(version.versioned_object_id().value().parse()?),
            kind: String::from("COMPOSITION"),
            sys_version: None,
            version_tree_id: None,
            change_type: change_type.to_owned(),
            template_id: None,
        }],
        envelope: serde_json::Map::new(),
    })
}

/// Returns the number of `cdm.measurement` rows.
async fn measurements(postgres: &containers::Postgres) -> Result<String, Box<dyn Error>> {
    let answered = rows(postgres, "SELECT count(*) FROM cdm.measurement").await?;
    Ok(answered
        .first()
        .and_then(|row| row.first())
        .ok_or("one cell")?
        .clone())
}

#[tokio::test]
async fn a_batch_run_after_the_incremental_load_leaves_the_cdm_as_it_was()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
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

    let mut seq = 0;
    for (ehr, version) in &committed {
        seq += 1;
        load.process(&event(seq, ehr, version, "249")?).await?;
    }
    load.settle().await?;
    assert_eq!("6", measurements(&postgres).await?);
    let incremental = crate::etl::dump(&postgres).await?;
    let batch = etl_run(&config).await?;
    assert_eq!(
        3, batch.compositions.committed,
        "the batch run commits every composition"
    );
    assert_eq!(
        incremental,
        crate::etl::dump(&postgres).await?,
        "a batch run after the incremental load writes the same rows"
    );

    let (doomed_ehr, doomed) = committed.get(1).ok_or("the second composition")?;
    match client.delete_composition(doomed_ehr, doomed).await?.outcome {
        CompositionDeleteOutcome::NoContent { .. } => {}
        other => return Err(format!("the CDR refused the delete: {other:?}").into()),
    }
    load.process(&event(seq + 1, doomed_ehr, doomed, "523")?)
        .await?;
    load.settle().await?;
    assert_eq!("4", measurements(&postgres).await?, "the deletion retracts");
    assert_eq!(0, crate::etl::orphans(&postgres).await?);
    let retracted = crate::etl::dump(&postgres).await?;
    let after = etl_run(&config).await?;
    assert_eq!(
        2, after.compositions.committed,
        "the batch run commits the compositions that stand"
    );
    assert_eq!(
        0,
        after.deletions.removed.len(),
        "the batch run finds no deletion the incremental load left"
    );
    assert_eq!(
        retracted,
        crate::etl::dump(&postgres).await?,
        "a batch run after the retraction writes the same rows"
    );
    Ok(())
}
