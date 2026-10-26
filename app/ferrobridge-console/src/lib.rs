// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroBRIDGE operator console: a Leptos app the server draws and the
//! browser hydrates whole, drawing every page after the first itself, and a
//! backend-for-frontend in front of the bridge.
//!
//! The console is one more client of the bridge's public HTTP surface: its
//! server reaches the bridge through [`bridge`], server to server, with the
//! signed-in operator's own access token, and never compiles or runs a
//! mapping itself. It holds the operator's sign-in session on the server
//! ([`session`], [`oidc`]), so the browser carries an opaque cookie and never
//! a token.
//!
//! The page tree is [`app`], drawn in the [`shell`] with the components of
//! `ferrohealth_ui` from the reviewed FerroBRIDGE design, its pages in
//! [`pages`], grouped into lazy routes by [`routes`], what they read from the
//! bridge in [`load`], what the server read of the request in [`visit`] and
//! the request's words in [`locale`]. Every page works with no WebAssembly:
//! each action is a link or a plain form the server answers ([`posts`],
//! [`prefs`]); once the page has hydrated, a form posts through its server
//! function ([`actions`]) and a link is followed in the page. [`editors`]
//! holds the console's own components. [`server`] serves it all under the browser security headers, [`config`]
//! reads the TOML configuration, [`cli`] and [`command`] run the binary, and
//! [`healthcheck`] is the job a container runtime runs beside it. No
//! specification governs the console: our own design.
//!
//! The server half compiles for the host and the browser half for
//! `wasm32-unknown-unknown`; the target, never a Cargo feature, chooses
//! between them.
#![doc(test(attr(deny(warnings))))]
// NOTE: the Rust reference, "Limits": the server's render of the library's
// sign-in card nests deeper than the default query depth of 128.
#![recursion_limit = "256"]

pub mod actions;
pub mod app;
pub mod editors;
pub mod load;
pub mod locale;
pub mod model;
pub mod pages;
pub mod routes;
pub mod shell;
pub mod visit;

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
pub mod oidc;
#[cfg(not(target_arch = "wasm32"))]
pub mod posts;
#[cfg(not(target_arch = "wasm32"))]
pub mod prefs;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;

/// Hydrates the page the server drew, whole, its lazy routes loading their
/// code from their own chunks.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_lazy(app::App);
}
