// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: both vendored mapping corpora load through the public
//! seam, and the pin matrix names the openEHR crate line this crate consumes.

#![allow(clippy::panic_in_result_fn, reason = "test assertions")]

/// A JSON snapshot with every object's members sorted by name, so it holds
/// whatever order the parsed map keeps: `serde_json` keeps insertion order only
/// under its `preserve_order` feature, which some builds of the workspace
/// turn on and others do not.
macro_rules! json_snapshot {
    ($($arg:tt)*) => {{
        let mut settings = insta::Settings::clone_current();
        settings.set_sort_maps(true);
        let _bound = settings.bind_to_scope();
        insta::assert_json_snapshot!($($arg)*)
    }};
}

mod adl2_fixtures;
mod alternatives;
mod anchors;
mod archetype_id;
mod corpus;
mod diagnostic;
mod embedded;
mod example;
mod header;
mod id_coded;
mod malformed;
mod named_sections;
mod ordinals;
mod party;
mod party_build;
mod paths;
mod pins;
mod registry;
mod relationship;
mod renamed;
mod terms;
mod web_template;
