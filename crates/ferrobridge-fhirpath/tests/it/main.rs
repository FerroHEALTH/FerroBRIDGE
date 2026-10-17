// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: the HL7 FHIRPath test suite as a conformance corpus,
//! the R4 core constraints, and the parser, evaluator and seams through the
//! public API.

#![allow(
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test assertions"
)]

mod check;
mod conformance;
mod constraints;
mod evaluate;
mod ordering;
mod parse;
mod seams;
mod suite;
mod support;
mod types;
mod ucum;
