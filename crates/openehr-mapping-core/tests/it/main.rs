// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: both vendored mapping corpora load through the public
//! seam, and the pin matrix names the openEHR crate line this crate consumes.

#![allow(clippy::panic_in_result_fn, reason = "test assertions")]

mod adl2_fixtures;
mod alternatives;
mod anchors;
mod archetype_id;
mod corpus;
mod diagnostic;
mod embedded;
mod header;
mod malformed;
mod ordinals;
mod party;
mod party_build;
mod paths;
mod pins;
mod registry;
mod terms;
mod web_template;
