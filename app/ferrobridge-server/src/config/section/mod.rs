// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The file tree: one struct per TOML section, each refusing an unknown key
//! and holding its defaults in its own `Default` impl.

pub mod audit;
pub mod cdm;
pub mod cdr;
pub mod change_feed;
pub mod etl;
pub mod events;
pub mod facade;
pub mod fhir_outbound;
pub mod hl7v2;
pub mod identity_source;
pub mod inbound;
pub mod mappings;
pub mod omop_incremental;
pub mod operations;
pub mod operator;
pub mod shl;
pub mod state;
pub mod terminology;
