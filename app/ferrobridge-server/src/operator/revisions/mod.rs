// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping-set revisions the operator API reads and applies, kept in the
//! state store ([`crate::mappings::revisions`]).
//!
//! A save and an apply record a revision that runs on every replica; an
//! import records one that waits for an apply. The provisioned files belong
//! to the deployment and are in no revision. No specification governs the
//! revisions: our own design.

pub mod apply;
pub mod archive;
pub mod diff;
pub mod exchange;
pub mod routes;
