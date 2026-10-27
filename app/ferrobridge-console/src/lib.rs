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
//! holds the console's own components. [`server`] serves it all under the
//! family's browser security headers, [`config`] reads the configuration
//! through `ferrohealth-config`, and [`cli`] and [`command`] run the binary
//! with the family's `serve`, `config check` and `healthcheck`. No
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
/// code from their own chunks, and marks the root element `data-hydrated`
/// once the page and its route have hydrated.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    ferrohealth_ui::widget::hydration::explain();
    // NOTE: leptos 0.8 `src/mount.rs`: `hydrate_lazy` runs this future and drops what
    // follows it; `hydrate_from_async` starts the executor itself.
    wasm_bindgen_futures::spawn_local(async {
        let Some(body) = leptos::prelude::document().body() else {
            leptos::logging::warn!("the page has no body to hydrate");
            return;
        };
        let page = leptos::mount::hydrate_from_async(body, app::App).await;
        page.forget();
        mark_hydrated();
    });
}

/// Marks the root element `data-hydrated` once the page and its lazy route
/// have hydrated, which a journey and the screenshot capture wait for before
/// they act.
///
/// The library's `data-ferroui-hydrated` (`widget::hydration::mark`) comes
/// from an `Effect` of the root component, which runs while a lazy route's
/// chunk is still on its way, so it can land before the route has hydrated.
#[cfg(target_arch = "wasm32")]
fn mark_hydrated() {
    if let Some(root) = leptos::prelude::document().document_element()
        && let Err(error) = root.set_attribute("data-hydrated", "")
    {
        leptos::logging::warn!("the browser refused the hydrated mark: {error:?}");
    }
}
