// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The documents of the FerroBRIDGE health and operator surfaces, as serde
//! types the bridge serializes and its operator console decodes.
//!
//! [`health`] holds `GET /health/info` and `GET /health/readiness`;
//! [`operator`] holds `GET /operator/state`, the `GET /operator/mappings`
//! listing, and the report a check, a save and a reload answer with;
//! [`configuration`] holds `GET /operator/configuration`, the running
//! configuration with every secret by reference alone; [`events`] holds
//! `GET /operator/events`, the change-feed consumers and the AMQP topology;
//! [`hl7v2`] holds `GET /operator/hl7v2/concept-maps`, the HL7 v2 face's
//! `ConceptMap` directory; [`capability`] holds
//! the facade's `CapabilityStatement` reduced to what the console draws;
//! [`problem`] holds the RFC 9457 problem details every operator refusal
//! answers with. A change to a document is a compile error on both sides,
//! never a field one side reads by a name the other stopped writing. The
//! crate holds types and nothing else. No specification governs these
//! surfaces: our own design.
#![doc(test(attr(deny(warnings))))]

pub mod capability;
pub mod configuration;
pub mod events;
pub mod health;
pub mod hl7v2;
pub mod operator;
pub mod problem;
