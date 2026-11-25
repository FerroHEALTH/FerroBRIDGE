// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR Mapping Language: the grammar of FHIR R4 `mapping.g4` and the
//! tutorial's rules (`docs/specs/fhir-mapping-language/`), the engine over
//! logical models, and a synthetic source rendered as an R4 `Patient`.

mod engine;
mod ons;
mod parse;
