// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests through the library run path the binary shares.

#![allow(
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test assertions"
)]
#![allow(
    clippy::indexing_slicing,
    reason = "indexing a serde_json::Value answers Null rather than panicking, and it is how a case reads a response body (<https://docs.rs/serde_json/1/serde_json/enum.Value.html#impl-Index%3CI%3E-for-Value>)"
)]

mod audit;
mod book_tutorial;
mod boot;
mod cdm;
mod cdr;
mod change_feed;
mod cli_reference;
mod commands;
mod config;
mod consent;
mod console;
mod etl;
mod events;
mod facade;
mod facade_bound_e2e;
mod facade_demographic_e2e;
mod facade_e2e;
mod facade_party_search_e2e;
mod facade_person_e2e;
mod facade_professional_e2e;
mod hl7v2;
mod http;
mod inbound;
mod ingest;
mod kds;
mod lane_templates;
mod mapping_check;
mod mapping_walk;
mod omop;
mod operations;
mod operator;
mod post_quantum;
mod quality;
mod readiness;
mod run;
mod search_e2e;
mod shutdown;
mod sources;
mod store;
mod structuremap;
mod support;
mod telemetry;
mod terminology_oauth;
mod usage_report;
mod yellow_button_e2e;
