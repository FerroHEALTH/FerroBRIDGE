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

#[cfg(feature = "r4")]
mod batch;
#[cfg(all(feature = "stu3", feature = "r4", feature = "r4b"))]
mod date;
#[cfg(any(feature = "r4", feature = "r4b"))]
mod end_to_end;
#[cfg(all(feature = "stu3", feature = "r4"))]
mod expand;
#[cfg(all(
    feature = "stu3",
    feature = "r4",
    feature = "r4b",
    feature = "r5",
    feature = "r6"
))]
mod forms;
#[cfg(feature = "r4")]
mod lookup;
#[cfg(feature = "r4")]
mod oauth;
mod pins;
#[cfg(all(feature = "r4", feature = "r4b", feature = "r5", feature = "r6"))]
mod releases;
#[cfg(all(feature = "stu3", feature = "r4", feature = "r4b"))]
mod search;
#[cfg(all(feature = "smart", feature = "r4"))]
mod smart;
#[cfg(feature = "stu3")]
mod stu3;
#[cfg(all(feature = "stu3", feature = "r4", feature = "r4b"))]
mod subsumes;
#[cfg(any(feature = "stu3", feature = "r4", feature = "r4b"))]
mod support;
#[cfg(feature = "r4")]
mod tls;
#[cfg(feature = "r4")]
mod translate;
#[cfg(feature = "r4")]
mod transport;
#[cfg(any(feature = "r4", feature = "r4b"))]
mod validate;
#[cfg(feature = "r4")]
mod validate_forms;
