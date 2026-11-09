// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIRconnect mapping language for Rust: the mapping file model and its
//! validation, context resolution into an immutable program, and the
//! bidirectional interpreter between openEHR compositions and FHIR resources.
//!
//! The language sits behind the default `mapping` feature. Without it the
//! crate is the FHIR path model alone ([`tree`]), over `fhir-types`, with no
//! openEHR crate.
#![doc(test(attr(deny(warnings))))]

#[cfg(feature = "mapping")]
pub mod engine;

#[cfg(feature = "mapping")]
pub mod model;

#[cfg(feature = "mapping")]
pub mod operations;

/// The FHIRconnect grammar this crate implements.
///
/// A mapping file writes it in its `grammar` header (<https://sevkohler.github.io/FHIRconnect-spec/build/site/FHIRconnect/v1.0.0/basics/main.html>).
pub const GRAMMAR: &str = "FHIRConnect/v1.0.0";

#[cfg(feature = "mapping")]
pub mod resolve;

pub mod tree;
