// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A composition deleted in the reference CDR after `etl run` loaded it,
//! behind the `FERROBRIDGE_E2E` gate.
//!
//! The round trip's three laboratory compositions are committed over
//! ITS-REST 1.1.0 and loaded; the second is then deleted with
//! `composition_delete`, which records a version whose data is Void (openEHR
//! RM Common 1.1.0 §6.2.5). The next run reads that composition with
//! `composition_get`, which answers `204` for a deleted composition, removes
//! its rows, and rebuilds the visit and the observation period of its EHR
//! from what is left. A further run changes nothing.

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::cdr::ids::{EhrId, versioned_object_uid};
use ferrobridge_testkit::containers;
use ferrobridge_testkit::fixtures::LABORATORY_REPORT_OPT;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_its::rest::generated::ehr::client::CompositionDeleteOutcome;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;
use std::error::Error;

use crate::omop_round_trip::{
    LABORATORIES, build_cdm, canonical, configuration, copy_mappings, create, create_ehr, etl_run,
    rows, upload_template,
};

/// Deletes the composition whose latest version is `latest`.
async fn delete(
    client: &CdrClient,
    ehr: &EhrId,
    latest: &ObjectVersionId,
) -> Result<(), Box<dyn Error>> {
    match client.delete_composition(ehr, latest).await?.outcome {
        CompositionDeleteOutcome::NoContent { .. } => Ok(()),
        other => Err(format!("the CDR refused the delete: {other:?}").into()),
    }
}

/// Returns the single cell `query` answers.
async fn cell(postgres: &containers::Postgres, query: &str) -> Result<String, Box<dyn Error>> {
    let answered = rows(postgres, query).await?;
    let row = answered.first().ok_or("one row")?;
    Ok(row.first().ok_or("one cell")?.clone())
}

#[tokio::test]
async fn a_composition_deleted_in_the_cdr_loses_its_rows_on_the_next_run()
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
    let mut versions = Vec::new();
    for laboratory in &LABORATORIES {
        let ehr = ehrs.get(laboratory.ehr).ok_or("an EHR")?;
        versions.push(create(&client, ehr, &canonical(&index, laboratory)?).await?);
    }
    let scratch = tempfile::tempdir()?;
    let mappings = tempfile::tempdir()?;
    copy_mappings(mappings.path())?;
    let config = configuration(scratch.path(), &cdr, &postgres, mappings.path())?;

    let first = etl_run(&config).await?;
    assert_eq!(3, first.compositions.committed, "{first}");
    assert_eq!(0, first.deletions.checked, "{first}");
    let ehr_1 = ehrs.first().ok_or("the first EHR")?.as_str().to_owned();
    let period_end = format!(
        "SELECT o.observation_period_end_date FROM cdm.observation_period o \
         JOIN ferrobridge.person_map p ON p.person_id = o.person_id WHERE p.ehr_id = '{ehr_1}'"
    );
    assert_eq!("2026-09-21", cell(&postgres, &period_end).await?);

    let doomed = versions.get(1).ok_or("the second version")?;
    let doomed_ehr = ehrs
        .get(LABORATORIES.get(1).ok_or("lab-2")?.ehr)
        .ok_or("an EHR")?;
    delete(&client, doomed_ehr, doomed).await?;
    let doomed_uid = versioned_object_uid(doomed).value().to_owned();

    let second = etl_run(&config).await?;
    assert_eq!(2, second.compositions.committed, "{second}");
    assert_eq!(1, second.deletions.checked, "{second}");
    assert_eq!(
        vec![doomed_uid.clone()],
        second.deletions.removed,
        "{second}"
    );
    assert_eq!(
        Some(&2),
        second.deletions.rows.get("measurement"),
        "{second}"
    );
    assert_eq!(
        "0",
        cell(
            &postgres,
            &format!(
                "SELECT count(*) FROM ferrobridge.record WHERE versioned_object_uid = '{doomed_uid}'"
            )
        )
        .await?
    );
    assert_eq!(
        "4",
        cell(&postgres, "SELECT count(*) FROM cdm.measurement").await?
    );
    assert_eq!(
        "2026-09-20",
        cell(&postgres, &period_end).await?,
        "the observation period no longer reaches the deleted composition's day"
    );
    let after_removal = crate::etl::dump(&postgres)?;

    let third = etl_run(&config).await?;
    assert_eq!(0, third.deletions.checked, "{third}");
    assert!(third.deletions.removed.is_empty(), "{third}");
    assert_eq!(
        after_removal,
        crate::etl::dump(&postgres)?,
        "a run after the removal changes nothing"
    );

    let last = versions.get(2).ok_or("the third version")?;
    let last_ehr = ehrs
        .get(LABORATORIES.get(2).ok_or("lab-3")?.ehr)
        .ok_or("an EHR")?;
    delete(&client, last_ehr, last).await?;
    let fourth = etl_run(&config).await?;
    assert_eq!(1, fourth.deletions.removed.len(), "{fourth}");
    assert_eq!(
        1, fourth.derived.visit_occurrence_removed,
        "the only composition of the second EHR's visit is gone: {fourth}"
    );
    assert_eq!(
        "1",
        cell(&postgres, "SELECT count(*) FROM cdm.visit_occurrence").await?
    );
    assert_eq!(
        "1",
        cell(&postgres, "SELECT count(*) FROM cdm.observation_period").await?,
        "the second EHR keeps no observation period"
    );
    Ok(())
}
