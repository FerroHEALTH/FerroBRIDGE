// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OMOP ETL runs `GET /operator/etl/runs` and
//! `GET /operator/etl/runs/{run_id}` answer.
//!
//! Each `etl run` and `etl fhir` keeps its report in the bridge schema once it
//! finished. The listing names each run with when it started and finished
//! and its counts; one run adds the rows it wrote by table, every record it
//! refused with the reason, and the compositions the deletion check removed,
//! kept or did not find. A run names compositions by their versioned object
//! id and counts rows; it never carries a clinical value. No specification
//! governs these documents: our own design.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What `GET /operator/etl/runs` answers with: the newest runs first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runs {
    /// The newest runs the bridge schema holds, newest first.
    pub runs: Vec<RunSummary>,
}

/// One run as the listing names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunSummary {
    /// The run's identifier.
    pub id: String,
    /// Which job ran.
    pub kind: RunKind,
    /// When it started, RFC 3339 UTC.
    pub started: String,
    /// When it finished, RFC 3339 UTC.
    pub finished: String,
    /// The compositions it read, and what became of them.
    pub compositions: Compositions,
    /// The rows it wrote over every table.
    pub rows: u64,
    /// The records it refused.
    pub refusals: u64,
    /// The compositions whose rows it removed because the CDR reports them
    /// deleted.
    pub removed: u64,
}

/// What `GET /operator/etl/runs/{run_id}` answers with: one run whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    /// The run's identifier.
    pub id: String,
    /// Which job ran.
    pub kind: RunKind,
    /// When it started, RFC 3339 UTC.
    pub started: String,
    /// When it finished, RFC 3339 UTC.
    pub finished: String,
    /// The compositions it read, and what became of them.
    pub compositions: Compositions,
    /// The rows it wrote, by CDM table.
    pub rows: BTreeMap<String, u64>,
    /// The `concept_id = 0` assignments it made.
    pub concept_zero: u64,
    /// Every record it refused.
    pub refusals: Vec<Refusal>,
    /// What the deletion check found.
    pub deletions: Deletions,
}

/// Which job a run was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    /// `etl run`: compositions read from the CDR.
    Compositions,
    /// `etl fhir`: FHIR resources read from a file or directory.
    Fhir,
}

/// The compositions one run read, and what became of them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compositions {
    /// Rows the composition query answered.
    pub read: u64,
    /// Compositions committed.
    pub committed: u64,
    /// Compositions skipped because their watermark names the same version.
    pub skipped: u64,
    /// Compositions refused whole.
    pub refused: u64,
}

/// One refused record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refusal {
    /// The versioned composition, when the refusal belongs to one.
    pub composition: Option<String>,
    /// The mapping.
    pub mapping: Option<String>,
    /// The CDM table.
    pub table: Option<String>,
    /// The CDM column.
    pub column: Option<String>,
    /// The openEHR element, as an RM path.
    pub element: Option<String>,
    /// Why.
    pub reason: String,
}

/// What the deletion check of one run found.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deletions {
    /// Compositions checked with the CDR.
    pub checked: u64,
    /// Checked compositions the CDR still holds.
    pub extant: u64,
    /// Compositions the CDR reports deleted, whose rows were removed.
    pub removed: Vec<String>,
    /// Compositions the CDR reports deleted, whose rows the `report` policy
    /// kept.
    pub kept: Vec<String>,
    /// Checked compositions the CDR does not know, whose rows are kept.
    pub unknown: Vec<String>,
    /// The rows removed, by CDM table.
    pub rows: BTreeMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::{Compositions, Deletions, Refusal, Run, RunKind};

    #[test]
    fn a_run_round_trips_through_its_json() {
        let run = Run {
            id: String::from("0b6c5a3e-1d3f-4c7e-9a43-000000000001"),
            kind: RunKind::Compositions,
            started: String::from("2026-11-03T09:00:00Z"),
            finished: String::from("2026-11-03T09:01:30Z"),
            compositions: Compositions {
                read: 3,
                committed: 2,
                skipped: 0,
                refused: 1,
            },
            rows: [(String::from("measurement"), 4)].into(),
            concept_zero: 1,
            refusals: vec![Refusal {
                composition: Some(String::from("8849182c-82ad-4088-a07f-48ead4180515")),
                mapping: None,
                table: None,
                column: None,
                element: None,
                reason: String::from("the composition names no template"),
            }],
            deletions: Deletions::default(),
        };
        let json = serde_json::to_value(&run).expect("the run serializes");
        assert_eq!(json["kind"], "compositions");
        let back: Run = serde_json::from_value(json).expect("the run reads back");
        assert_eq!(back, run);
    }
}
