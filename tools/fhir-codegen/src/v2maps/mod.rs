// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The v2-to-FHIR root set: the guide's `ConceptMap`s in, the structure maps
//! of `ferrobridge-hl7v2` out.
//!
//! The guide `hl7.fhir.uv.v2mappings` 1.0.0 ships four kinds of map, named by
//! the first word of their id. The message, segment and data type maps are
//! the bridge's mapping logic, so every row of them is emitted as a static: the root set is every map
//! whose [`read::Kind`] is not [`read::Kind::Table`]. The table maps are terminology, which a
//! terminology server answers by `ConceptMap/$translate`; of them only the
//! header is emitted, the id, the canonical url and each group's source and
//! target system, which is what a translation names. FerroBRIDGE's structure
//! corrections are `ConceptMap`s in the guide's shape under
//! `overrides/hl7.fhir.uv.v2mappings/`, and its value maps are the FHIR
//! package under `terminology/hl7v2-values/` an operator loads into the
//! terminology server; both are emitted beside the guide's, never merged
//! into them, so the crate can run the guide alone.
//!
//! The pipeline is [`read`] (decode one directory), [`render`] (source text)
//! and [`emit`] (write or check).

pub mod emit;
pub mod read;
pub mod render;
