// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: the package set, canonical resolution, each structural
//! check over a synthetic resource, the extension checks, the report and its
//! `OperationOutcome`, and a real profile of the vendored EU packages.

#![allow(
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test assertions"
)]

mod bindings;
mod bundles;
mod cardinality;
mod eu;
mod extensions;
mod outcome;
mod packages;
mod resolve;
mod scope;
mod slicing;
mod structure;
mod support;
mod values;
