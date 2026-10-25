// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: the crate's pinned specification version agrees with
//! the pin matrix, the vendored mapping library runs through all three
//! validation layers, and one context of it compiles into one program.

#![allow(clippy::panic_in_result_fn, reason = "test assertions")]

mod corpus;
mod derive;
mod engine;
mod eu_condition;
mod eu_laboratory;
mod eu_patient;
mod event_time;
mod events;
mod kds_coding;
mod laborauftrag;
mod laws;
mod library;
mod locations;
mod model;
mod operations;
mod person;
mod pins;
mod published;
mod resolve;
mod roundtrip;
mod support;
mod term_mapping;
mod tree;
