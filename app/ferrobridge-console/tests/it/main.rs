// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The integration tests of the operator console, one binary with a module
//! per subject.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::indexing_slicing,
    reason = "test assertions"
)]

mod boundary;
mod bridge;
mod config;
mod configuration;
mod editor;
mod faces;
mod files;
mod forms;
mod mappings;
mod pages;
mod playground;
mod refresh;
mod run;
mod server;
mod shell;
mod sign_in;
mod sign_out;
mod stub;
mod support;
