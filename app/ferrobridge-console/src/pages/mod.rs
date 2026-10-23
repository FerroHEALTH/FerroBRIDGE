// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's pages, each drawn whole on the server from what it reads
//! from the bridge, as the reviewed FerroBRIDGE design lays it out
//! (FerroUI `design/consoles/ferrobridge/`).
//!
//! Every page is built from the templates and components of
//! `ferrohealth_ui`: a list page (T1), a detail page (T2), a settings or
//! reference page (T3) or the About page, with the dashboard and the two
//! workbenches (the editor and the playground) as the design's exceptions.
//! [`parts`] holds what several pages draw alike. No specification governs
//! the console's pages: our own design.

pub mod about;
pub mod connections;
pub mod dashboard;
pub mod editor;
pub mod errors;
pub mod facade;
pub mod file;
pub mod hl7v2;
pub mod mappings;
pub mod omop;
pub mod parts;
pub mod playground;
pub mod preferences;
pub mod search;
pub mod sign_in;
pub mod template;
