<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrobridge-viewer

The operator console. The Leptos discipline is `.claude/rules/leptos-ui.md`,
which loads for every file here; this file is what is particular to the
crate.

- **One crate, two targets, no `ssr`/`hydrate` feature.** The host build is
  the server half, the `wasm32-unknown-unknown` build the browser half; a
  module for one half carries `#[cfg(target_arch …)]`.
- **The boundary is a test.** The server half links `openehr-mapping-core`,
  `fhir-types` (`r4`) and `ferrobridge-operator-api` of the workspace and
  nothing else; the browser half links none. The bridge's health and
  operator documents decode into the contract crate's types, never by field
  name. `tests/it/boundary.rs` walks the graph `cargo metadata`
  resolves, and its injected-edge cases prove it refuses another crate. A
  new workspace dependency here is an architecture change, returned to the
  orchestrator.
- **`main.rs` never grows.** It calls `command::run(std::env::args())`; the
  tests drive `run`, `Config::from_sources`, `router` and `Bridge`.
- **`crate::bridge` owns every call to the bridge**, and a refusal keeps the
  bridge's status and body (`BridgeError::Refused`).
- **Every configuration default lives inline in its struct's `Default`
  impl**, with container-level `#[serde(default, deny_unknown_fields)]`. A
  refusal names the key, never the value; a secret goes through
  `config::secret::resolve`.
- **The session is the server's** (`session`, `oidc`): the browser holds
  one opaque cookie, and a token, a code, a PKCE verifier or a client
  secret never reaches a page, a cookie, a `Debug` rendering or a log
  line; `tests/it/sign_in.rs` and `tests/it/refresh.rs` read the captured
  log and the page for each. Sign-in, refresh and sign-out are tested
  against the test OpenID Provider of `ferrobridge_testkit::oidc`.
- **Every server function checks the session first**
  (`session::request::signed_in`) and is listed in
  `tests/it/server_functions.rs`, which fails on one it does not hold.
- **A view paints with roles, never palette entries** (`styles`,
  `style/tailwind.css`): the role layer derives from
  `assets/brand/tokens.css`, both dark blocks stay identical, and
  `tests/it/contrast.rs` holds every pair to WCAG 2.2 AA. The theme is the
  server's, drawn from its cookie as `data-theme`; the browser keeps no
  storage.
- **The security headers are tested in-process** (`tests/it/server.rs`): a
  new route keeps the CSP nonce, `nosniff`, `DENY`, `no-referrer` and
  `no-store` on documents, and only `/pkg/` is compressed.
