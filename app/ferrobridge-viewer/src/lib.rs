// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroBRIDGE operator console: a Leptos app rendered on the server and
//! hydrated in the browser, with a backend-for-frontend in front of the
//! bridge.
//!
//! The console is one more client of the bridge's public HTTP surface: its
//! server reaches the bridge through [`bridge`], server to server, with the
//! signed-in operator's own access token, and never compiles or runs a
//! mapping itself. It holds the operator's sign-in session on the server
//! ([`session`], [`oidc`]), so the browser carries an opaque cookie and never
//! a token. [`app`] is the page tree both halves share, drawn in the
//! [`shell`] with the class vocabulary of [`styles`] in the operator's
//! [`theme`], with the maker named on the [`about`] page, and filled through
//! the server functions of [`status`] with the view models of [`model`];
//! [`server`] serves it with the health and sign-in routes under the browser
//! security headers, [`config`] reads the TOML configuration, [`cli`] and
//! [`command`] run the binary, and [`healthcheck`] is the job a container
//! runtime runs beside it. No specification governs the console: our own
//! design.
//!
//! The server half compiles for the host and the browser half for
//! `wasm32-unknown-unknown`; the target, never a Cargo feature, chooses
//! between them.
#![doc(test(attr(deny(warnings))))]

pub mod about;
pub mod app;
// The browser half of a server function, which the server macro writes, is a
// network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod engine;
// The browser half of a server function, which the server macro writes, is a
// network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod editor;
pub mod model;
pub mod shell;
pub mod styles;
// The browser half of a server function, which the server macro writes, is a
// network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod theme;
// The browser half of a server function, which the server macro writes, is a
// network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod tools;
// The browser half of a server function, which the server macro writes, is a
// network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod status;

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
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;

/// Attaches the browser half to the page the server rendered.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}
