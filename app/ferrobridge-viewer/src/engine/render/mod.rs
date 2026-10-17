// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The engine-state screens drawn as HTML on the server.
//!
//! Each screen is read-only output, so the server draws it whole with the
//! same `view!` it would hydrate, and hands the browser the HTML: the
//! diagnostics read into the mapping foundation's own type never cross into
//! the browser half, which links no workspace crate. Every text is escaped by
//! the renderer. No specification governs the console: our own design.

pub mod capability;
pub mod dashboard;
pub mod mappings;
pub mod parts;
