// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Each OMOCL target beside `Measurement` and `Person`, as one isolated case:
//! a synthetic mapping over the laboratory template writes one row of its CDM
//! table, with the columns the projection gives its keys
//! (`model::projection`, our own key-to-column table over the OMOCL syntax
//! tables and the CDM v5.4 field definitions).
//!
//! The concept keys write the literal concept `0`, so no case needs a
//! vocabulary row; the dates are the event time each analyte falls back to,
//! and the numbers the analyte's `DV_QUANTITY`.

use std::error::Error;

use omocl::engine::RefusalKind;
use omop_cdm::graph::RecordGraph;
use omop_cdm::graph::row::Row;

use crate::lab::StubSource;
use crate::lab::composition;
use crate::lab::flat;
use crate::lab::template;
use crate::resolve::ANALYTE;
use crate::resolve::RESULT;
use crate::resolve::mapping;

use crate::engine::cell;
use crate::engine::float;
use crate::engine::int;
use crate::engine::outcome;
use crate::engine::rows;

/// The event time every analyte of the fixture falls back to.
const EVENT_DATE: &str = "2026-09-20";

/// Runs one `target` mapping of `keys` over `archetype` of the fixture
/// composition, refusing nothing.
async fn run_target(
    archetype: &str,
    target: &str,
    keys: &str,
) -> Result<RecordGraph, Box<dyn Error>> {
    let stub = StubSource::laboratory()?;
    let index = template()?;
    let composition = composition(&index, &flat(&[], &[])?)?;
    let file = mapping(
        "Synthetic_target_v1",
        archetype,
        &format!("  - type: \"{target}\"\n{keys}"),
    );
    let (graph, refusals) = outcome(
        &[("Synthetic_target_v1.yml", &file)],
        &index,
        &composition,
        &stub,
    )
    .await?;
    assert!(refusals.is_empty(), "{target}: {refusals:?}");
    Ok(graph)
}

/// Returns a key of one path alternative.
fn path(key: &str, path: &str) -> String {
    format!("    {key}:\n      alternatives:\n        - path: \"{path}\"\n")
}

/// Returns a key of one literal concept.
fn code(key: &str, code: i32) -> String {
    format!("    {key}:\n      alternatives:\n        - code: {code}\n")
}

/// Returns the date cell of `column`.
fn date(row: &Row, column: &str) -> Option<String> {
    row.cell(column).map(cell)
}

#[tokio::test]
async fn an_observation_writes_its_concept_date_and_value() -> Result<(), Box<dyn Error>> {
    let keys = [
        code("concept_id", 0),
        path("observation_date", "../../"),
        path("value", "/items[at0001]"),
    ]
    .concat();
    let graph = run_target(ANALYTE, "Observation", &keys).await?;
    let found = rows(&graph, "observation");
    let values: Vec<Option<f64>> = found
        .iter()
        .map(|row| float(row, "value_as_number"))
        .collect();
    assert_eq!(vec![Some(7.25), Some(140.0)], values);
    for row in found {
        assert_eq!(Some(0), int(row, "observation_concept_id"));
        assert_eq!(Some(EVENT_DATE.to_owned()), date(row, "observation_date"));
    }
    Ok(())
}

#[tokio::test]
async fn a_drug_exposure_writes_its_dates_and_quantity() -> Result<(), Box<dyn Error>> {
    let keys = [
        code("concept_id", 0),
        path("drug_exposure_start_date", "../../"),
        path("drug_exposure_end_date", "../../"),
        path("quantity", "/items[at0001]"),
    ]
    .concat();
    let graph = run_target(ANALYTE, "DrugExposure", &keys).await?;
    let found = rows(&graph, "drug_exposure");
    let quantities: Vec<Option<f64>> = found.iter().map(|row| float(row, "quantity")).collect();
    assert_eq!(vec![Some(7.25), Some(140.0)], quantities);
    for row in found {
        assert_eq!(Some(0), int(row, "drug_concept_id"));
        assert_eq!(
            Some(EVENT_DATE.to_owned()),
            date(row, "drug_exposure_start_date")
        );
        assert_eq!(
            Some(EVENT_DATE.to_owned()),
            date(row, "drug_exposure_end_date")
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_condition_occurrence_writes_its_concept_and_start() -> Result<(), Box<dyn Error>> {
    let keys = [
        code("concept_id", 0),
        path("condition_start_date", "../../"),
    ]
    .concat();
    let graph = run_target(ANALYTE, "ConditionOccurrence", &keys).await?;
    let found = rows(&graph, "condition_occurrence");
    assert_eq!(2, found.len());
    for row in found {
        assert_eq!(Some(0), int(row, "condition_concept_id"));
        assert_eq!(
            Some(EVENT_DATE.to_owned()),
            date(row, "condition_start_date")
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_procedure_occurrence_writes_its_concept_and_date() -> Result<(), Box<dyn Error>> {
    let keys = [code("concept_id", 0), path("procedure_date", "../../")].concat();
    let graph = run_target(ANALYTE, "ProcedureOccurrence", &keys).await?;
    let found = rows(&graph, "procedure_occurrence");
    assert_eq!(2, found.len());
    for row in found {
        assert_eq!(Some(0), int(row, "procedure_concept_id"));
        assert_eq!(Some(EVENT_DATE.to_owned()), date(row, "procedure_date"));
    }
    Ok(())
}

#[tokio::test]
async fn a_device_exposure_writes_its_concept_and_start() -> Result<(), Box<dyn Error>> {
    let keys = [
        code("concept_id", 0),
        path("device_exposure_start_date", "../../"),
    ]
    .concat();
    let graph = run_target(ANALYTE, "DeviceExposure", &keys).await?;
    let found = rows(&graph, "device_exposure");
    assert_eq!(2, found.len());
    for row in found {
        assert_eq!(Some(0), int(row, "device_concept_id"));
        assert_eq!(
            Some(EVENT_DATE.to_owned()),
            date(row, "device_exposure_start_date")
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_specimen_writes_its_concept_date_and_quantity() -> Result<(), Box<dyn Error>> {
    let keys = [
        code("concept_id", 0),
        path("specimen_date", "../../"),
        path("quantity", "/items[at0001]"),
    ]
    .concat();
    let graph = run_target(ANALYTE, "Specimen", &keys).await?;
    let found = rows(&graph, "specimen");
    let quantities: Vec<Option<f64>> = found.iter().map(|row| float(row, "quantity")).collect();
    assert_eq!(vec![Some(7.25), Some(140.0)], quantities);
    for row in found {
        assert_eq!(Some(0), int(row, "specimen_concept_id"));
        assert_eq!(Some(EVENT_DATE.to_owned()), date(row, "specimen_date"));
    }
    Ok(())
}

#[tokio::test]
async fn a_visit_writes_its_concept_and_both_dates() -> Result<(), Box<dyn Error>> {
    let event = "/data[at0001]/events[at0002]";
    let keys = [
        code("visit_concept", 0),
        path("start_datetime", event),
        path("end_datetime", event),
    ]
    .concat();
    let graph = run_target(RESULT, "Visit", &keys).await?;
    let found = rows(&graph, "visit_occurrence");
    let [row] = found.as_slice() else {
        panic!("one VISIT_OCCURRENCE row, got {}", found.len());
    };
    assert_eq!(Some(0), int(row, "visit_concept_id"));
    assert_eq!(Some(EVENT_DATE.to_owned()), date(row, "visit_start_date"));
    assert_eq!(Some(EVENT_DATE.to_owned()), date(row, "visit_end_date"));
    Ok(())
}

#[tokio::test]
async fn a_death_is_refused_until_the_graph_keys_it_by_its_person() -> Result<(), Box<dyn Error>> {
    // TODO(#1208): DEATH has no surrogate key, so the graph refuses the row;
    // the case asserts the one DEATH row once the person keys it.
    let stub = StubSource::laboratory()?;
    let index = template()?;
    let composition = composition(&index, &flat(&[], &[])?)?;
    let file = mapping(
        "Synthetic_target_v1",
        RESULT,
        &format!(
            "  - type: \"Death\"\n{}",
            path("death_date", "/data[at0001]/events[at0002]")
        ),
    );
    let (graph, refusals) = outcome(
        &[("Synthetic_target_v1.yml", &file)],
        &index,
        &composition,
        &stub,
    )
    .await?;
    assert_eq!(0, rows(&graph, "death").len());
    let [refusal] = refusals.as_slice() else {
        panic!("one refusal, got {refusals:?}");
    };
    assert_eq!(RefusalKind::InvalidValue, refusal.kind);
    assert!(
        refusal.message.contains("no single integer primary key"),
        "{refusal:?}"
    );
    Ok(())
}
