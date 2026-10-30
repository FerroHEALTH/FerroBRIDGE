<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrobridge-console

The operator console. The console discipline is `.claude/rules/console-ui.md`,
which loads for every file here; this file is what is particular to the
crate.

- **One crate, one target, every page drawn on the server.** No Leptos, no
  WebAssembly, no site bundle: `cargo build` makes the one binary, which
  serves its pages, FerroUI's stylesheet and theme, and FerroUI's script
  modules (`ferrohealth_ui::serve::axum_router`).
- **The boundary is a test.** The console links `openehr-mapping-core`,
  `fhir-types` (`r4`) and `ferrobridge-operator-api` of the workspace and
  nothing else, and no Leptos crate or WebAssembly glue. The bridge's health
  and operator documents decode into the contract crate's types, never by
  field name. `tests/it/boundary.rs` walks the graph `cargo metadata`
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
- **A request is one `visit::Visit`, a page one document.** `server::page`
  reads the request into a `Visit` (the path and query, the session, the
  operator, the preference cookies without the session's, the locale, and
  the `load::Reads` that ask the bridge each document once), `routes::draw`
  picks the page its path names behind the session's guard, and
  `app::render` puts the `app::Drawn` page in the document and the
  `shell`. A post to a page that takes one goes to `routes::posted`: Reload,
  the editor and the playground draw their page again with the bridge's
  answer (`actions` holds what they post, `posts` their work), and every
  other route answers `405`. `tests/it/forms.rs` lists every post and the
  pages that take none, and holds each to a live session from the
  console's own pages.
- **A view draws with `ferrohealth-ui`** (FerroUI, pinned `=0.6.0`): its
  components, templates, script modules and brand theme, never a local
  copy or override; a gap is a FerroUI issue and a `TODO(#N)` naming its
  tracking issue here. Markup of the console's own is escaped parts
  (`markup::h` and `markup::t`). Each preference is the server's, drawn
  from its own cookie (`ferrobridge_console_<name>`, one year) onto
  `<html>`; the theme is set in the user menu and Your preferences alone,
  and the browser keeps no storage. Every string is a `Msg` of
  `locales/en/ferrobridge-console.ftl`, numbers through the registered
  `format_number`.
- **A route has a journey and two screenshots.** The browser journeys and
  the book's screenshots live in the testkit
  (`tools/ferrobridge-testkit/tests/it/journeys/`); a route of
  `routes::ROUTES` joins the shots of `journeys::screenshots`, whose guard
  fails without its light and dark image on the book's operator console
  page. Capture them with `scripts/release/journey-tools.sh
  target/journey-tools` and `FERROBRIDGE_SCREENSHOTS=1`.
- **The security headers are tested in-process** (`tests/it/server.rs`): a
  new route keeps the strict policy (`script-src 'self'; style-src 'self'`,
  no nonce), `nosniff`, `DENY`, `no-referrer` and `no-store` on documents,
  and nothing is compressed.
