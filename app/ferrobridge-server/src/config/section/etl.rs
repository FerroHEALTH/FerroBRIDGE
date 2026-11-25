// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[etl]` section.
//!
//! No specification governs the configuration: our own design.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The OMOP ETL job.
///
/// The queries are parsed and checked when the configuration loads, so a
/// query the runner cannot read refuses the start of any job.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Etl {
    /// The AQL that selects each composition whole, aliased `ehr_id`,
    /// `versioned_object_uid`, `version_uid` and `composition`.
    pub aql: String,
    /// How many rows one page of a query asks for.
    pub page_size: u32,
    /// The `*_type_concept_id` the mappings write; no default, because it
    /// records a provenance only the deployment knows.
    pub type_concept_id: Option<i32>,
    /// The `period_type_concept_id` of every observation period.
    pub observation_period_type_concept_id: Option<i32>,
    /// The visit derivation, off until its section is present.
    pub visits: Option<EtlVisits>,
    /// What a run does with a composition the CDR reports deleted: `remove`
    /// or `report`.
    pub deletions: String,
    /// The OHDSI `vocabulary_id` each terminology id names, a URI or an OID
    /// among them, added over the engine's defaults.
    pub vocabulary_aliases: BTreeMap<String, String>,
    /// Whether `etl run` runs the ported Data Quality Dashboard checks over
    /// the tables it wrote once the load is done.
    pub quality_checks: bool,
    /// Which rows of the derived tables a run rebuilds: `owned_persons`, the
    /// rows of the persons the bridge writes, or `whole_table`.
    pub derived_tables: String,
}

impl Default for Etl {
    fn default() -> Self {
        Self {
            aql: String::new(),
            page_size: 100,
            type_concept_id: None,
            observation_period_type_concept_id: None,
            visits: None,
            // NOTE: no specification governs this: our own design; keeping the rows of a
            // record the source deleted is the defect, so a confirmed deletion removes them.
            deletions: String::from("remove"),
            vocabulary_aliases: BTreeMap::new(),
            quality_checks: false,
            // NOTE: no specification governs this: our own design; another ETL's derived rows
            // survive a run unless the deployment says the bridge is the only writer.
            derived_tables: String::from("owned_persons"),
        }
    }
}

/// The visit derivation of the OMOP ETL job.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct EtlVisits {
    /// The AQL aliased `ehr_id`, `visit_source`, `visit_start` and
    /// `visit_end`.
    pub aql: String,
    /// The `visit_concept_id` every visit carries.
    pub visit_concept_id: Option<i32>,
    /// The `visit_type_concept_id` every visit carries.
    pub visit_type_concept_id: Option<i32>,
}
