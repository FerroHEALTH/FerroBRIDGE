// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroBRIDGE operator console: a Leptos app rendered on the server and
//! hydrated in the browser, with a backend-for-frontend in front of the
//! bridge.
//!
//! The console is one more client of the bridge's public HTTP surface: its
//! server reaches the bridge through [`bridge`], server to server, and never
//! compiles or runs a mapping itself. [`app`] is the page tree both halves
//! share; [`server`] serves it with the health route under the browser
//! security headers, [`config`] reads the TOML configuration, [`cli`] and
//! [`command`] run the binary, and [`healthcheck`] is the job a container
//! runtime runs beside it. No specification governs the console: our own
//! design.
//!
//! The server half compiles for the host and the browser half for
//! `wasm32-unknown-unknown`; the target, never a Cargo feature, chooses
//! between them.
#![doc(test(attr(deny(warnings))))]

pub mod app;

#[cfg(not(target_arch = "wasm32"))]
pub mod bridge;
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod command;
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod healthcheck;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;

/// Attaches the browser half to the page the server rendered.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}
