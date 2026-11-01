// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroBRIDGE operator console: every page drawn whole on the server
//! from FerroUI's components, and a backend-for-frontend in front of the
//! bridge.
//!
//! The console is one more client of the bridge's public HTTP surface: its
//! server reaches the bridge through [`bridge`], server to server, with the
//! signed-in operator's own access token, and never compiles or runs a
//! mapping itself. It holds the operator's sign-in session on the server
//! ([`session`], [`oidc`]), so the browser carries an opaque cookie and never
//! a token.
//!
//! A request is read once into a [`visit::Visit`]: its path and query, its
//! session, its preferences and locale ([`locale`]), and the reads it makes
//! of the bridge ([`load`]). [`routes`] picks the page its path names from
//! [`pages`], each an axum handler's work that draws the page whole from the
//! components of `ferrohealth_ui`, and [`app`] puts it in the document and
//! the [`shell`]. Every page works with no script: a link loads a document,
//! a preference is a plain form the server answers with a redirect
//! ([`prefs`]), and Reload, the editor and the playground post to their own
//! page, which draws the bridge's answer ([`actions`], their work in
//! [`posts`]); [`editors`] draws the library's code editor for the two
//! workbenches. FerroUI's script modules add behaviour on top. [`server`]
//! serves it all under the family's browser security headers, [`config`]
//! reads the configuration through `ferrohealth-config`, and [`cli`] and
//! [`command`] run the binary with the family's `serve`, `config check` and
//! `healthcheck`. No specification governs the console: our own design.
#![doc(test(attr(deny(warnings))))]

pub mod actions;
pub mod app;
pub mod bridge;
pub mod cli;
pub mod command;
pub mod config;
pub mod editors;
pub mod load;
pub mod locale;
pub mod markup;
pub mod model;
pub mod oidc;
pub mod pages;
pub mod posts;
pub mod prefs;
pub mod routes;
pub mod server;
pub mod session;
pub mod shell;
pub mod visit;
