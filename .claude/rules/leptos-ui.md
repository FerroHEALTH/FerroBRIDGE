---
paths: ["app/ferrobridge-console/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Leptos console rules (`app/ferrobridge-console`, and any Leptos code)

**FerroUI leads the UI.** The console draws with `ferrohealth-ui`, pinned
exactly in the root `[workspace.dependencies]` (today `=0.3.6`, with
its `split` feature on in the console crate), and the
FerroUI repository (FerroHEALTH/FerroUI) is the source for its components,
design, tokens, icons, accessibility bar, interaction contract, words and
quality control. Read these first, at the git tag that matches the pin
(today `v0.3.6`):

- [`docs/consoles.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.3.6/docs/consoles.md):
  the consumer guide, how a console uses the library.
- [`.claude/rules/leptos-ui.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.3.6/.claude/rules/leptos-ui.md):
  the component rules of every FerroHEALTH console. The section numbers this
  file cites (FerroUI §N) are those of the `v0.3.6` copy.
- [`crates/ferrohealth-ui/CLAUDE.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.3.6/crates/ferrohealth-ui/CLAUDE.md):
  the crate's own discipline.
- [`design/consoles/ferrobridge/`](https://github.com/FerroHEALTH/FerroUI/tree/v0.3.6/design/consoles/ferrobridge):
  this console's pages, at 1440 and 390 pixels in both themes, which the
  screenshots are checked against.

The console moved to 0.3.2 with FerroUI #161 (#1143): islands mode, the
four-layer interaction contract, the templates and components of
`design/COMPONENTS.md`, preferences in cookies of their own. FerroUI's
`design/` is the only design source; a design folder anywhere else is stale
and is never synced from. The console moved to 0.3.3 with #1154, which
drew the parts the rollout had found missing (#1146), and to 0.3.4 with
#1155, which drew the flow's last two (its glyphs and How it is drawn), and to 0.3.5 with #1166 and 0.3.6 with #1168, which it
follows with no change of its own. A component this console needs and the library
lacks, or a component that behaves wrongly, is an issue filed in
FerroHEALTH/FerroUI and linked from the work here: never a local copy, a
shadow component, a stylesheet override or a hand-written script in this
repository.

This file keeps only what is particular to the operator console: its routes
and pages, the BFF and its session, the form posts, the bridge client
and the boundary, the configuration and security headers, the bundle budget
measurement, the journeys and screenshots, and the console's own words. The
console's shape is section 4.10 of `docs/architecture.md`. The oracle for
every Leptos question is the official Leptos book
(<https://github.com/leptos-rs/book>, `main`, which targets Leptos 0.8),
never memory; citations below are book chapters (`view/04_iteration`,
`ssr/23_ssr_modes`) and pinned crate documentation.

**The console runs in Leptos islands mode** (the book's "Guide: Islands"),
with a backend-for-frontend (BFF) on the same server, built by cargo-leptos
with `--split`, which is why the console turns on `ferrohealth-ui`'s `split`
feature (FerroUI `docs/consoles.md`, "How a console builds"). The server
draws every page whole; the browser hydrates the
islands alone: the library's (the menus, the search, the dialogs) and the
console's two, `MappingEditor` and `JsonInput`, both `#[island(lazy)]` so
each loads in a chunk of its own. The page tree (`app`, `shell`, `pages`)
is server-only and compiles for the host alone. The two halves are chosen by
**compilation target, never by Cargo feature**: the host build is the server
half (`leptos/ssr`, axum, the bridge client), the `wasm32-unknown-unknown`
build is the browser half (`leptos/hydrate`, `islands`). The
crate has no `ssr` or `hydrate` feature, so a workspace `--all-features`
build never turns both on. A change that brings back an `ssr`/`hydrate`
feature pair is an architecture change, returned to the orchestrator, never
made in a slice.

## 0. The mandates (absolute)

- **Rust only, zero hand-written JavaScript, no inline style**: FerroUI §0.
  What this console adds: the book's `oninput="this.form.requestSubmit()"`
  trick in `router/20_form` is forbidden here (use an `on:` Rust listener),
  the only JavaScript in the product is the `wasm-bindgen` glue and the
  hydration bootstrap the toolchain generates, and there is no npm, no Node
  and no CSS build: the server serves the library's stylesheet and the
  console's brand theme.
- **The console is a client of the bridge and nothing else.** Its server
  reaches the bridge over HTTP, on the public surface any client uses,
  through `crate::bridge`, server to server. Of the workspace's own crates
  the server half links `openehr-mapping-core` (the `Diagnostic` a check
  answers), `fhir-types` with `r4` (the `OperationOutcome` and the
  `CapabilityStatement` it renders) and `ferrobridge-operator-api` (the
  bridge's health and operator documents, which the bridge serializes with
  the same types) and nothing else; the browser half links
  none and receives its islands' props in the page the server draws. `tests/it/boundary.rs`
  walks the resolved graph and fails on any other workspace crate, so the
  console never compiles, transforms or saves a mapping itself: every check,
  transform and save goes through the bridge. A screen that needs what the
  bridge does not expose gets an endpoint of the bridge's operator API first,
  as its own issue, behind the bridge's token validation.
- **The console stores no clinical data, and holds no token in the
  browser.** The transform playground shows the operator what a `$tofhir` or
  `$toopenehr` answered, and nothing keeps it: no browser storage (local or
  session storage, IndexedDB), no service worker, no cache (every page, the
  answer to a form post among them, is `Cache-Control: no-store`, held by a
  test), and no URL. The operator's session is server-side; the browser
  carries one opaque `HttpOnly` session cookie. An access token, a client
  secret, or a PKCE verifier never reaches a signal, a prop, a serialized
  resource, the page HTML, or a log.
- **No patient identifier in a URL, the browser history, or a log.** A
  patient is named through a request body or a form the BFF reads, never a
  path segment or a query parameter the router or the browser keeps, and a
  test over the rendered output and the request log proves it for each
  screen that takes one.
- **No server function; every action is a plain form post.** A page's
  form posts to an axum route of `prefs` or `posts`, which checks the
  session itself before it touches the bridge or session state, and answers
  with a redirect (a preference) or with the page drawn again by
  `server::render` with the outcome in context (a reload, a check, a save, a
  run). Every action works before any WebAssembly arrives. The server's
  same-origin check passes safe methods only, so an action is never a
  `GET`. `tests/it/forms.rs` lists every post and holds each to a live
  session from the console's own pages before it asks the bridge or sets a
  cookie, and asserts `server_fn_paths()` is empty. A new post joins that
  list in the change that adds it.

## 1. Crate and build discipline

- One crate, two targets. Server-only dependencies live under
  `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`, browser-only
  ones under `[target.'cfg(target_arch = "wasm32")'.dependencies]`, and code
  for one half only carries the matching `#[cfg(target_arch …)]`. Keep the
  lib compiling for `wasm32-unknown-unknown` at all times: the CI `console`
  job runs clippy on that target.
- **A server function is an architecture change**, returned to the
  orchestrator: the console's actions are form posts (§0).
- **WebAssembly is 32-bit: never `usize`/`isize` in an island's props or in
  any serialized shared type.** Fixed-size integers only (`server/25`
  quirks).
- Signals hold `Send + Sync` values, because the server renders on a
  multi-threaded tokio runtime. The `_local` variants (`signal_local`,
  `LocalResource`, `Action::new_local`) are only for genuinely `!Send`
  browser types; a `LocalResource` where a serializable `Resource` works is
  a deoptimization under SSR (`reactivity/working_with_signals`, `ssr/23`).
- **Bundle size.** The bundle builds under the root `Cargo.toml`
  `[profile.wasm-release]` (`opt-level = "z"`, `lto = true`,
  `codegen-units = 1`), selected by `lib-profile-release` in the crate's
  `[package.metadata.leptos]`; `[profile.release]` stays the server's. Avoid
  `regex` and generics-heavy code on browser paths. A new browser-side
  dependency is justified against the bundle bytes it adds, measured from
  `scripts/release/console-site.sh --release`, and the bundle is held to the
  budget of §12.
- **The bundle name is fixed at compile time.** Leptos names the WebAssembly
  file it loads from `LEPTOS_OUTPUT_NAME`, which the repository's
  `.cargo/config.toml` sets to the crate's `output-name`, so a plain cargo
  build and a cargo-leptos build render the same page. The server test
  `a_page_carries_a_fresh_nonce_its_scripts_use_and_is_never_stored_or_compressed`
  holds it.
- **Views are built in `.into_any()`-erased sections.** A monolithic `view!`
  tree over deeply nested components blows rustc's layout-recursion depth at
  codegen in a plain `cargo build`. Break every screen into section functions
  bound to erased locals.
- The workspace bar applies unchanged (`.claude/rules/reliability.md`,
  `.claude/rules/rust-style.md`): no `unsafe`, no `unwrap`/`expect` outside
  tests, `thiserror` for the console's error enums, every public item
  documented, suppressions as `#[expect(lint, reason = "…")]` scoped to the
  smallest item. The `#[component]` macro drops attributes such as
  `#[must_use]` from the function it writes, so a public component carries a
  scoped `#[expect(clippy::must_use_candidate, reason = "…")]` instead.

## 2. Reactivity

The reactivity rules are FerroUI §3 (setup functions run once, the access
discipline, no signal written from an `Effect` that reads another). Beside
them (`reactivity/working_with_signals`, `15_global_state`):

- Never `sig.get().is_empty()`, which clones the whole value, and never hold
  a `.read()` guard across a write.
- A signal derived from a signal is a derived closure or a `Memo`. Effects
  only sync with the non-reactive outside world.
- Prefer local component state. Escalate in this order: the URL (router),
  then a context signal, then a `Store`. Context values use the newtype
  pattern; `expect_context` only where provision is structurally guaranteed.

## 3. Components and props

The prop rules are FerroUI §3: a changing value is a `Signal<T>` prop, and a
change reports back through a callback prop or an `on:` listener. Every
component and every prop carries a doc comment, and a `WriteSignal` goes
down only where it is needed.

## 4. Views: iteration, control flow and errors

The view rules are FerroUI §4: a `<For>` key that is stable and
data-derived, never an index; an error that never renders as nothing; valid
HTML; `rel="external"` on an anchor to a BFF axum route. What is particular
here:

- A `<For>` key of this console is a mapping file's path, a context name, or
  a diagnostic's position and code.
- A bridge refusal renders its status and the `OperationOutcome` or
  diagnostics it carried, in the section whose data failed.
- Expensive branches go behind `<Show when fallback>`; divergent branch types
  use `Either` or `.into_any()`.
- **Never create a resource inside a `Suspend` closure.** A `Suspend` re-runs
  on every notification of what it awaits, and each re-run re-creates what
  is inside it; resource ids then diverge between the server pass and
  hydration, and hydration reads the wrong serialized slot.

## 5. Forms

The form rules are FerroUI §5: a stable `id` and `name` on every control
with its label's `for`, a `<textarea>` filled through `prop:value` alone
(the hazard measured in #608), and no `GET` form for a field that can carry
clinical content. What is particular here:

- Every form is a plain `<form method="post">` to its own route, drawn
  whole on the server with its values, and works with no WebAssembly. An
  island inside it (the editor's text area, the playground's input) is a
  real control with its `name`, so the browser posts what it holds.
- The decision of a form with several buttons is its submit button's
  `name="decision"` and `value` (the editor: `check`, `save`, `confirm`,
  `back`, `discard`), and the post draws the page again with the answer.
- A text area submits its line breaks as CRLF (HTML Living Standard
  §4.10.11); the editor's post reads the draft back with LF, so a save is
  the bytes the editor showed.
- Parse user input yourself. `type="number"` is not validation.
- The playground's composition or Bundle can carry a patient identifier, so
  its form is a `POST` (§0).

## 6. Async data (the BFF pattern)

- A page reads the bridge on the server, in a `Suspense` over
  `Suspend::new(async { … })`, through `crate::load`, whose per-request
  `Reads` cache asks the bridge once for a document the shell and the page
  both draw (`async/11_suspense`).
- A mutation is a form post (§0), never a fetch from the browser.
- Only the visible tab reads.

## 7. The server half

- **One module owns every bridge call: `crate::bridge`.** It builds each
  request with the operator's token and reads the answer into the
  workspace's typed bodies (`fhir-types`, `openehr-mapping-core`,
  `ferrobridge-operator-api`), never reading a document by field name. Never a
  second client, a shadow type for a body the workspace already models, or a
  `reqwest` call from a page or a post (`.claude/rules/codegen.md`,
  `.claude/rules/spec-adherence.md`).
- A post handler is thin: check the session, call `crate::bridge`, render.
  Logic lives in plain modules with ordinary unit tests.
- Errors are typed: the bridge's `StatusCode` and body are carried as data
  (`BridgeError::Refused`), and a non-2xx answer is never an empty value.
- **An expected outcome is drawn as a page.** A read or a post yields a
  `Loaded<T>`: the document, or a bridge refusal with its status
  (`Outcome::Refused`), or the console's own fault (`ViewError`: a bridge it
  cannot reach, a body it cannot read). The page draws each in the section
  whose data it is. A caller branches on the three, so they are distinct
  types (`.claude/rules/reliability.md`, errors are types at a boundary that
  branches).
- Shared state reaches a render through `leptos_routes_with_context` and
  `provide_context`, and the plain axum handlers through `Extension`
  (`server/26_extractors`).
- **The security headers are the server's**, on every answer: a
  Content-Security-Policy with a nonce minted per response and no
  `'unsafe-inline'` or `'unsafe-eval'` for scripts, `nosniff`, `DENY`
  framing, no referrer, and `no-store` on every document. A new inline
  script or style that needs the policy relaxed is the defect, not the
  policy. `tests/it/server.rs` holds every header in-process.

## 8. SSR and hydration correctness (`ssr/22` to `ssr/24`)

- An island runs on both halves, so its server HTML and its hydrated view
  must be identical. Never branch an island's view **structure** on
  `cfg!(target_arch = "wasm32")`. Everything an island draws arrives as a
  prop the server resolved, the words included.
- Browser-only calls (`window`, storage, timers) go inside `Effect::new` in
  an island; everything else is server-only.
- Valid HTML and a deterministic first render are FerroUI §4; hydration
  walks the DOM, so a breach of either is a hydration error here.
- **A route whose content comes from the bridge renders `SsrMode::Async`**
  (`ssr/23_ssr_modes`): the server waits for every resource and sends the
  whole page, so nothing arrives in a `<template>` for an inline script to
  swap in, and the page reads with no script.
- `leptos_meta` (`<Title>`, `<Stylesheet>`, `<Meta>`) is used from component
  bodies, never by editing the shell's `<head>` by hand (`metadata`). Every
  routed page sets a `<Title>`.

## 9. Router

- One `<Router>` at the root and `<Routes fallback=…>` with a real 404 that
  sets the status (`router/16_routes`).
- Params and queries are typed through `use_params::<T>()` and
  `use_query::<T>()` with `#[derive(Params)]`; handle the `Err` and `None`
  cases, which are user input (`router/18_params_and_queries`).
- Filter, search and pagination state lives in the URL (`router/20_form`),
  except a value that can identify a patient (§0).
- A param a same-route navigation can change is read reactively, never
  `get_untracked` at setup.

## 10. Accessibility

The bar is FerroUI §7: **WCAG 2.2 Level AA** (<https://www.w3.org/TR/WCAG22/>)
in the light and the dark theme, keyboard operable with a visible focus, no
meaning on colour alone, reduced motion respected, the sticky top bar never
covering the focus (SC 2.4.11). A view uses the components and the role
classes of `ferrohealth-ui`, never a colour of its own. What is particular
here, checked per slice:

- Tables carry real `<th scope=…>` headers, and a status is a word as well
  as a tint.
- The brand theme is the library's (`BRAND.theme()`), whose contrast
  FerroUI's own tests hold; the console adds no colour.
- The top bar stays in view from `md` up, so the stylesheet's
  `scroll-padding-top` clears it for a focused control.

## 11. Testing and gates

- **Business logic lives outside components**, in plain types with ordinary
  unit tests. Components stay thin.
- The HTTP surface is tested in-process: `crate::server::router` driven with
  `tower::ServiceExt::oneshot` (`tests/it/server.rs`), and the bridge client
  against a `wiremock` stub bridge (`tests/it/bridge.rs`), with synthetic
  bodies.
- Gates for every console change: `cargo fmt` clean; `cargo clippy -p
  ferrobridge-console --all-targets -- -D warnings` on the host and `cargo
  clippy -p ferrobridge-console --lib --target wasm32-unknown-unknown -- -D
  warnings`; `cargo nextest run -p ferrobridge-console`;
  `scripts/release/console-site.sh --release` completing when the change
  touches the build surface.
- **Browser journeys** drive the console in headless Chrome through a
  WebDriver client in Rust, never Playwright, which is JavaScript and falls
  under the no-JavaScript mandate. They live in
  `tools/ferrobridge-testkit/tests/it/journeys/` behind
  `FERROBRIDGE_JOURNEYS=1`, run against a running bridge and console in the
  `journeys (browser)` CI job, and fail on any `SEVERE` entry the browser
  logs. A new screen or route gets its journey and its light and dark
  screenshots (`FERROBRIDGE_SCREENSHOTS=1`, shown on the book's operator
  console page) in the same change; the screenshot guard of the ordinary
  suite fails on a route without both.
- **Never weaken a gate to make a change pass.** A failing wasm32 clippy pass
  usually means a dependency cannot compile for the browser, which is the
  gate working.

## 12. The bundle budget and the `wasm-release` profile

The browser downloads the WebAssembly before the page hydrates, so its size
is held to a budget in CI. After `scripts/release/console-site.sh --release`,
the `console` job runs `scripts/checks/console-bundle.sh`, which writes the
WebAssembly and its JavaScript glue, raw, `gzip -9` and `brotli -q 11`, to
the job summary, and fails when the brotli-compressed WebAssembly is over the
budget below. The script reads the budget from this table, so the number
changes here and nowhere else.

| Measure | Budget |
|---|---|
| WebAssembly, brotli-compressed | 458752 bytes |

- **The gating number is the brotli-compressed WebAssembly the page loads
  first.** The book's `deployment/binary_size` chapter has a site serve its
  WebAssembly compressed, every current browser accepts brotli, and the
  compressed size is the download. The raw and gzip sizes are reported beside
  it, ungated, and so are the lazy island chunks `--split` writes, which a
  page loads only when it draws that island.
- **The budget is 448 KiB.** The move to FerroUI 0.3.6 (#1168, 2026-10-24,
  the same tools) measured 323191 bytes for the entry, 70% of it, with
  294668 more in the lazy chunks: 221197 in the five `split_*` files and
  73471 in the thirteen `chunk_*` files. The move to FerroUI 0.3.5 (#1166, 2026-10-24,
  the same tools) measured 323039 bytes for the entry, 70% of it, with
  294581 more in the lazy chunks: 221118 in the five `split_*` files and
  73463 in the thirteen `chunk_*` files. The move to FerroUI 0.3.4 (#1155, 2026-10-23,
  cargo-leptos 0.3.11, wasm-bindgen 0.2.129, Rust 1.99.1) measured 322256
  bytes for the entry, 70% of it, with 294857 more in the lazy chunks
  (#1162): 221301 in the five `split_*` files of the islands' own code and
  73556 in the thirteen `chunk_*` files they share. The move to FerroUI 0.3.3 with its `split`
  feature (#1154, 2026-10-23, the same tools) measured 319953 bytes for the
  entry, 69% of it, with 294802 more in the lazy chunks. The move to FerroUI
  0.3.2 in islands mode
  (#1143, 2026-10-23, cargo-leptos 0.3.11, wasm-bindgen 0.2.129, Rust
  1.98.1) measured 316603 bytes for the entry, 69% of it, with 292391 more
  in the lazy chunks of the two console islands and the library's. Built
  whole, without `--split`, it measured 520589, over the budget: the code
  editor and its YAML and JSON tokenizers are most of a chunk, so the site
  script always builds split. Before it, 448 KiB was 25% over the 366571
  bytes the move to
  `ferrohealth-ui` 0.1.7 measured (#956, 2026-10-19, cargo-leptos 0.3.11,
  wasm-bindgen 0.2.129, Rust 1.98.1), 51871 bytes over 0.1.6's 314700: every
  word the components and the console draw now comes from a Fluent catalog,
  so the bundle carries the Fluent parser that reads the embedded catalogs
  once, the CLDR plural rules a catalog selects a form by, and the
  catalogs. FerroUI's gallery measured the same cost (53325 bytes). Its
  history:
  - 314 KiB, 25% over the 257500 bytes the mapping editor of #608
    measured (2026-10-17, cargo-leptos 0.3.11, wasm-bindgen 0.2.129,
    Tailwind CSS 4.3.3, Rust 1.98.1).
  - 112 KiB, 26% over the 90964 bytes of the scaffold of #599: the router,
    the meta tags and the hydration of a two-route shell.
  - 192 KiB for the sign-in of #600, which measured 157809. 54766 of the
    66845 bytes it added are the first server function read under
    `<Suspense>`, measured by building the page without it (103043): the
    server function client, its JSON codec and the typed refusal it
    decodes, which every later screen that reads the bridge carries.
  - 244 KiB for the shell of #601, which added 42238 bytes: the top bar and
    its status read under a second `<Suspense>`, the router links of the
    grouped navigation that mark the current page, the About route, and the
    drawer's two browser behaviours (the focus moved into it, and closing it
    on a followed link), which measured 4420 bytes of it. Its theme switch
    adds nothing to the bundle: it is a plain form post the server answers
    with a redirect to the same page in the new theme. An action that
    switched in place measured 12411 bytes more, and was left out.
  - Within 244 KiB, the read-only screens drawn on the server cost the
    browser little: the engine-state screens of #604 measured 198283, the
    mapping files of #607 203059. The playground and the dry run of #605,
    three `ServerAction`s over controlled forms, measured 234423.
  - 314 KiB for the mapping editor of #608, which added 23077 bytes: three
    more `ServerAction`s (check, save, reload), the confirmation checkbox,
    the save's digest carried between saves, and the `Effect` that moves the
    focus to the save's result. Every answer is still drawn on the server.
  - Within 314 KiB, the move to `ferrohealth-ui` 0.1.1 (#850) measured
    297091, and its highlighted editor 311080 (2026-10-18). The editor
    draws the draft through the library's YAML tokenizer in the browser as
    you type, so it costs about 14 KB. The file view uses the same
    tokenizer and so always agrees with it.
  - Within 314 KiB, `ferrohealth-ui` 0.1.2 measured 315175 and 0.1.3 315639
    (2026-10-19): their stacked tables, chevron select and nested-reveal
    rules are stylesheet, and the data table they stack is the one the
    console already drew. 0.1.4 measured 315522 (2026-10-19): its
    timestamp is drawn on the server, and its compact tile and stacking
    rules are stylesheet. 0.1.5 measured 315756 (2026-10-19): its per-row
    field and header title are thin wrappers the console does not call. 0.1.6
    measured 314700 (2026-10-19): its field shapes are drawn on the server
    and the console's fields already use the shared field.

  The headroom is the quarter FerroFED's console started with, enough for
  toolchain drift and small growth and too little for a slice to pass
  unmeasured. The JavaScript glue measured 24217 bytes raw and 6008
  brotli-compressed. A slice that needs more raises the budget in this
  table in its own pull request, with the measured size and the reason,
  after checking that what it adds belongs in the browser at all (§1).
  Server-rendered HTML for read-only output stays the preferred shape.
- **The profile keeps the overflow checks.** The console's own browser code
  does little arithmetic; the checks guard the arithmetic of Leptos, serde
  and the standard library, where a wrapped length or index would draw a
  wrong page with no error, and FerroFED measured the saving of dropping
  them at 1.5%, too little to justify an exception to
  `.claude/rules/reliability.md`.
- **The profile drops the line tables** (`debug = false`). wasm-bindgen
  strips the debug sections from the bundle it writes, so the inherited
  `line-tables-only` costs build time alone. A browser panic still names its
  file and line: `console_error_panic_hook` prints the panic location, which
  rustc embeds as data whatever the debug setting.
- **The bundle is served compressed.** The server compresses a response
  whose media type is the bundle's (`application/wasm`, JavaScript, CSS) with
  brotli or gzip, as `Accept-Encoding` chooses, and marks it
  `Vary: Accept-Encoding`, through the `tower-http` compression layer
  (`deployment/binary_size`). A document, the answer to a form post among
  them, is never compressed: it can carry what the operator entered beside data an
  attacker would want, which a compression side channel (BREACH) could read.
  `tests/it/server.rs` holds both.
- **The bundle names no directory of the build host.** The panic locations
  rustc embeds as data name the source file of every crate, so an
  unremapped CI build ships `/home/runner/...` and the registry and
  toolchain paths. `scripts/release/console-site.sh` remaps the cargo home,
  the toolchain sysroot and the checkout to `/cargo`, `/rustc-sysroot` and
  `/ferrobridge` for the WebAssembly build alone
  (`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS` with
  `--remap-path-prefix`; `trim-paths` is unstable), and the `console` job's
  `scripts/checks/console-paths.sh` fails when a home, runner, registry or
  toolchain path appears in the WebAssembly or its JavaScript glue. A
  `RUSTFLAGS` set in the environment overrides the target variable, and the
  check then fails.
- **The build tools are pinned and checked.** `scripts/release/console-tools.sh`
  installs Binaryen's `wasm-opt` at the release `docs/VERSIONS.md` pins,
  checked against its SHA-256, and `console-site.sh` refuses to run
  cargo-leptos until the one on `PATH` matches, so cargo-leptos never
  downloads it unchecked. The CI job installs cargo-leptos and the
  wasm-bindgen CLI at their pinned versions; the CLI equals the
  `wasm-bindgen` crate version, and `scripts/checks/versions.sh` holds all
  three to the pin matrix.
- **`panic = "unwind"`, inherited from `release`, does not apply to
  `wasm32-unknown-unknown`.** The target's standard library is built to
  abort on a panic, so a panic in the browser prints its message and
  location through `console_error_panic_hook` and stops the module; the
  `catch_unwind` that turns a server panic into a `500`
  (`.claude/rules/reliability.md`) has no browser counterpart. Keep the
  browser code free of panicking paths for that reason.

## 13. Icons

The icon rule is FerroUI §9: `ferrohealth_ui::icon::Glyph`, the Lucide
outlines the library vendors, each beside the word it illustrates, and no
icon crate of the console's own. A glyph the console needs and the library lacks is a
FerroUI issue.

## 14. Strings and locales

The catalog rule is FerroUI's (its `.claude/rules/leptos-ui.md` §13): no
string the console draws lives outside a catalog, and a language joins as a
catalog file with no code change. What is particular here:

- The console's catalog is `app/ferrobridge-console/locales/<tag>/ferrobridge-console.ftl`,
  embedded by `fluent_templates::static_loader!` in `crate::locale`, where
  each message is one `Msg`. A test holds every catalog directory to the
  full message set, and a throwaway catalog under `tests/locales/` shows the
  words change with no code change.
- The server chooses the locale once per request (`crate::locale::requested`):
  the language of the operator's language cookie when the catalog holds it,
  else the best language of `Accept-Language` the catalog holds, else
  `[server] locale`. `App` provides it with
  `ferrohealth_ui::i18n::provide_locale`, so the console's words and the
  library's switch together, the document writes it as `<html lang>`, and an
  island receives its words resolved as props.
- Numbers go through the `format_number` formatter registered in the
  `static_loader!` customise hook, with the catalog's `number-group` and
  `number-decimal` messages.
- `scripts/checks/stray-literals.sh app/ferrobridge-console/src` runs in CI
  against `scripts/checks/stray-literals.baseline`, which names no file: a
  console string outside the catalog fails it. A literal no page draws (a
  protocol token, a header value, the name of a document in a log line) is
  allowed in `scripts/checks/stray-literals.allow` with its reason, and the
  files that speak only to the terminal (`cli.rs`, `command.rs`, `config/`)
  are skipped there.
- A fault or a refusal the console draws reads its words from the catalog
  (`ViewError::text`, `Refusal::text`, `Shape::text`), and a plain-text
  answer of an axum route reads them in the locale `crate::locale::negotiated`
  picks from the request's headers. An error's `Display` is for the log
  alone, and never reaches a page.
