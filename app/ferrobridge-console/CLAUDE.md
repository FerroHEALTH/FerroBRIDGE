<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrobridge-console

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
- **Islands mode, no server function.** The page tree is server-only; the
  browser half compiles `islands` alone (`MappingEditor`, `JsonInput`, both
  `#[island(lazy)]`, and the library's islands). Every action is a plain
  form post (`prefs`, `posts`) that checks the session first and answers
  with the page `server::render` draws, the outcome in context;
  `tests/it/forms.rs` lists every post, holds each to a live session from
  the console's own pages, and asserts no server function is registered.
- **A view draws with `ferrohealth-ui`** (FerroUI, pinned `=0.3.3`): its
  templates, components and brand theme (`BRAND.theme()`), never a local
  copy or override; a gap is a FerroUI issue and a `TODO(#1155)`. Each
  preference is the server's, drawn from its own cookie
  (`ferrobridge_console_<name>`, one year) onto `<html>`; the theme is set
  in the user menu and Your preferences alone, and the browser keeps no
  storage. The server serves the library's stylesheet, the theme and the
  marks itself (`server::stylesheet`, `server::theme`). Every string is a
  `Msg` of `locales/en/ferrobridge-console.ftl`, numbers through the
  registered `format_number`.
- **A route has a journey and two screenshots.** The browser journeys and
  the book's screenshots live in the testkit
  (`tools/ferrobridge-testkit/tests/it/journeys/`); a new route joins the
  shots of `journeys::screenshots`, whose guard fails without its light and
  dark image on the book's operator console page. Capture them with
  `scripts/release/journey-tools.sh target/journey-tools`, the release site
  bundle, and `FERROBRIDGE_SCREENSHOTS=1`.
- **The security headers are tested in-process** (`tests/it/server.rs`): a
  new route keeps the CSP nonce, `nosniff`, `DENY`, `no-referrer` and
  `no-store` on documents, and only `/pkg/` is compressed.
