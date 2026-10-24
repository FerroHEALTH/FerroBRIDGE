// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: the five terminology operations and the code system
//! count search against a stub server,
//! the batch form, the STU3 wire version, the transport rules, a run against
//! the reference server in a container, and the crate's pinned specification
//! version.

#![allow(
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test assertions"
)]

mod batch;
mod date;
mod end_to_end;
mod expand;
mod lookup;
mod oauth;
mod pins;
mod search;
mod stu3;
mod subsumes;
mod support;
mod translate;
mod transport;
mod validate;
mod validate_forms;
