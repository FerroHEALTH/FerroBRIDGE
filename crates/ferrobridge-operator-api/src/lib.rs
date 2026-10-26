// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The documents of the FerroBRIDGE health and operator surfaces, as serde
//! types the bridge serializes and its operator console decodes.
//!
//! [`health`] holds `GET /health/info` and `GET /health/readiness`;
//! [`operator`] holds `GET /operator/state`, the `GET /operator/mappings`
//! listing, and the report a check, a save and a reload answer with;
//! [`configuration`] holds `GET /operator/configuration`, the running
//! configuration with every secret by reference alone; [`capability`] holds
//! the facade's `CapabilityStatement` reduced to what the console draws. A
//! change to a document is a compile error on both sides, never a field one
//! side reads by a name the other stopped writing. The crate holds types and
//! nothing else. No specification governs these surfaces: our own design.
#![doc(test(attr(deny(warnings))))]

pub mod capability;
pub mod configuration;
pub mod health;
pub mod operator;
