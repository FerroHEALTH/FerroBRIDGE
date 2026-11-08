// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: every rule of the FHIR R4 search value grammar through
//! the public API, against the examples of search.html and hostile input.

#![allow(
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test assertions"
)]

mod date;
mod escape;
#[cfg(feature = "jiff")]
mod instant;
mod prefix;
mod string;
mod token;
