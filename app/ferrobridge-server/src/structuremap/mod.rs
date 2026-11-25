// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR Mapping Language: a map parsed from its text and run over a
//! source instance into FHIR R4 (FHIR R4 §7.7, mapping-language.html, and
//! the `StructureMap` resource, structuremap.html).
//!
//! A source that sends its own JSON is described by a logical model, a
//! `StructureDefinition` of `kind = logical`, and a map turns an instance of
//! it into R4 resources. The engine holds no knowledge of any source.

pub mod engine;
pub mod expression;
pub mod model;
pub mod openapi;
pub mod parse;
pub mod syntax;
