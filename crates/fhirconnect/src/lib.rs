// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIRconnect mapping language for Rust: the mapping file model and its
//! validation, context resolution into an immutable program, and the
//! bidirectional interpreter between openEHR compositions and FHIR resources.
#![doc(test(attr(deny(warnings))))]

pub mod engine;

pub mod model;

pub mod operations;

/// The FHIRconnect grammar this crate implements.
///
/// A mapping file writes it in its `grammar` header (<https://sevkohler.github.io/FHIRconnect-spec/build/site/FHIRconnect/v1.0.0/basics/main.html>).
pub const GRAMMAR: &str = "FHIRConnect/v1.0.0";

pub mod resolve;

pub mod tree;
