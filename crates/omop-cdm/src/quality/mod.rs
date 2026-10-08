// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OHDSI Data Quality Dashboard's checks, ported to run against a CDM
//! v5.4 PostgreSQL database (Blacketer et al., JAMIA 2021,
//! doi:10.1093/jamia/ocab132).
//!
//! [`definitions`] is the vocabulary the generated catalogue and settings are
//! written in ([`crate::generated::dqd`]). [`check`] names the ported checks
//! and one check instance, [`plan`] decides which instances exist for the
//! CDM, [`sql`] renders each instance as a PostgreSQL query, and [`evaluate`]
//! turns the counts a query returns into a result with the Dashboard's
//! verdict. With the `database` feature, `run` executes a plan on a
//! `CdmPool`.
//!
//! The structural facts a check reads (required, datatype, keys, domain,
//! class) come from the CDM v5.4.3 definitions through
//! [`crate::generated::TABLES`]; the thresholds, the notes and the enablement
//! of `isStandardValidConcept` come from the Dashboard's files. The SQL is
//! FerroBRIDGE's own PostgreSQL translation of the Dashboard's SQL Server
//! templates; no specification governs that translation: our own design.

pub mod check;
pub mod definitions;
pub mod evaluate;
pub mod plan;
#[cfg(feature = "database")]
pub mod run;
pub mod sql;

/// The tag of `OHDSI/DataQualityDashboard` the catalogue and the settings are
/// emitted from.
///
/// The files are vendored at this tag (`docs/specs/dqd/PROVENANCE.md`).
pub const DQD_TAG: &str = "v2.9.0";
