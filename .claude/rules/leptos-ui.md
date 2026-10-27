---
paths: ["app/ferrobridge-console/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Leptos console rules (`app/ferrobridge-console`, and any Leptos code)

**FerroUI leads the UI.** The console draws with `ferrohealth-ui`, pinned
exactly in the root `[workspace.dependencies]` (today `=0.5.1`), and the
FerroUI repository (FerroHEALTH/FerroUI) is the source for its components,
design, tokens, icons, accessibility bar, interaction contract, words and
quality control. Read these first, at the git tag that matches the pin
(today `v0.5.1`):

- [`docs/consoles.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.5.1/docs/consoles.md):
  the consumer guide, how a console uses the library.
- [`.claude/rules/leptos-ui.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.5.1/.claude/rules/leptos-ui.md):
  the component rules of every FerroHEALTH console. The section numbers this
  file cites (FerroUI §N) are those of the `v0.5.1` copy.
- [`crates/ferrohealth-ui/CLAUDE.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.5.1/crates/ferrohealth-ui/CLAUDE.md):
  the crate's own discipline.
- [`design/consoles/ferrobridge/`](https://github.com/FerroHEALTH/FerroUI/tree/v0.5.1/design/consoles/ferrobridge):
  this console's pages, at 1440 and 390 pixels in both themes, which the
  screenshots are checked against.

The console moved to 0.3.2 with FerroUI #161 (#1143): islands mode, the
four-layer interaction contract, the templates and components of
`design/COMPONENTS.md`, preferences in cookies of their own. FerroUI's
`design/` is the only design source; a design folder anywhere else is stale
and is never synced from. The console moved to 0.3.3 with #1154, which
drew the parts the rollout had found missing (#1146), and to 0.3.4 with
#1155, which drew the flow's last two (its glyphs and How it is drawn), and to 0.3.5 with #1166 and 0.3.6 with #1168, which it
follows with no change of its own. It moved to 0.5.0 with #1256, which
hydrates the whole page and draws every page after the first in the
browser (FerroUI #410), and to 0.5.1, whose shell is drawn once
and follows the router (`widget::nav::follow_path`) and whose
`widget::hydration` marks and explains the hydration (`docs/consoles.md`
steps 9 and 10). A component this console needs and the library
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

**The console hydrates the whole page** (FerroUI `docs/consoles.md`, "How a
console builds", and FerroUI §15), with a backend-for-frontend (BFF) on the
same server, built by cargo-leptos with `--split`. The server draws the
first page, the browser takes the whole tree over (`hydrate_lazy`) and
draws every page after it, so no link, tab, search or action reloads the
document; the browser's back and forward walk the pages it drew. The pages
sit in five lazy route groups (`crate::routes`: Overview, Mappings,
Workbench, Bridge, Console), each a chunk of its own that a page loads the
first time it draws one of its pages and `widget::prefetch::routes` loads
while the browser is idle. The two halves are chosen by **compilation
target, never by Cargo feature**: the host build is the server half
(`leptos/ssr`, axum, the bridge client), the `wasm32-unknown-unknown`
build is the browser half (`leptos/hydrate`). Neither turns on the
`leptos` feature `islands`. The crate has no `ssr` or `hydrate` feature, so
a workspace `--all-features` build never turns both on. A change that
brings back an `ssr`/`hydrate` feature pair is an architecture change,
returned to the orchestrator, never made in a slice.

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
  `CapabilityStatement` it reads) and `ferrobridge-operator-api` (the
  bridge's health and operator documents, which the bridge serializes with
  the same types) and nothing else; the browser half links
  `openehr-mapping-core` and `ferrobridge-operator-api` alone, never the
  FHIR model: a page draws the contract's `capability::Capability`, the
  facade's `CapabilityStatement` the server reduced. `tests/it/boundary.rs`
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
- **Every action works with no WebAssembly, and reloads nothing with it.**
  A page's form posts to an axum route of `prefs` or `posts`, which checks
  the session itself before it touches the bridge or session state, and
  answers with a redirect (a preference) or with the page drawn again by
  `server::render` with the outcome in context (a reload, a check, a save, a
  run): that is the page with no WebAssembly. Once the page has hydrated,
  the form's own `on:submit` prevents the browser's post and sends the same
  work through its server function in `crate::actions` (`reload`, `edit`,
  `run`), and the page draws the answer in place; a preference form is the
  library's, marked `widget::submit::NAVIGATES`, which `FormNavigator`
  posts in the background. The form keeps its route as `action`, because
  the console answers `Referrer-Policy: no-referrer`, so a plain post to a
  server function's own address could not be sent back to its page. That
  is a stopgap the owner accepted: the forms move to `<ActionForm>` once
  FerroUI fixes its Referer-bound fallback (FerroUI #449, here #1265). Every
  read is a server function of `crate::load`, and the visit (`crate::visit`)
  one more. Every server function checks the session before it asks the
  bridge, and the same-origin check refuses an unsafe method from anywhere
  else. `tests/it/forms.rs` lists every post and every server function,
  holds each to a live session from the console's own pages, asserts the
  registered set, and holds the visit to carry no session cookie and no
  token. A new post or server function joins those lists in the change
  that adds it.

## 1. Crate and build discipline

- One crate, two targets. Server-only dependencies live under
  `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`, browser-only
  ones under `[target.'cfg(target_arch = "wasm32")'.dependencies]`, and code
  for one half only carries the matching `#[cfg(target_arch …)]`. Keep the
  lib compiling for `wasm32-unknown-unknown` at all times: the CI `console`
  job runs clippy on that target.
- **A new server function is a reviewed change**: it reads through
  `crate::load` or acts through `crate::actions`, checks the session first,
  and joins `tests/it/forms.rs` (§0). Its answer is `no-store`, as every
  answer outside `/pkg/` is.
- **WebAssembly is 32-bit: never `usize`/`isize` in a serialized shared
  type** (a server function's arguments and answer, the visit, a
  `SharedValue`). Fixed-size integers only (`server/25` quirks).
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
  whole on the server with its values, and works with no WebAssembly. Once
  hydrated its `on:submit` reads the form with `FromFormData::from_event`,
  the submitter's `name` and `value` included, and dispatches the action's
  server function; a form that does not read falls through to the
  browser's post. A component inside it (the editor's text area, the
  playground's input, `crate::editors`) is a real control with its `name`,
  so the browser posts what it holds.
- The decision of a form with several buttons is its submit button's
  `name="decision"` and `value` (the editor: `check`, `save`, `confirm`,
  `back`, `discard`), and the post draws the page again with the answer.
  In the page, Discard follows the link back to the file, and every other
  decision goes through the `edit` server function.
- A text area submits its line breaks as CRLF (HTML Living Standard
  §4.10.11); the editor's post and its server function read the draft back
  with LF, so a save is the bytes the editor showed.
- Parse user input yourself. `type="number"` is not validation.
- The playground's composition or Bundle can carry a patient identifier, so
  its form is a `POST` (§0), and its server function takes it in the
  request body.
- A post sent before the WebAssembly arrives is drawn by the server with
  its answer, and the page carries that answer as a `SharedValue`, so the
  browser hydrates the page the server drew.

## 6. Async data (the BFF pattern)

- A page reads the bridge through a server function of `crate::load`
  under a `Resource` it creates at its setup, never inside a `Suspend`
  (§4), keyed on `load::changes`, and draws it in a `Suspense` over
  `Suspend::new(async { … })` (`async/11_suspense`). In the server's own
  render the server function is a plain call into `load::cached`, whose
  per-request `Reads` cache asks the bridge once for a document the shell
  and the page both draw; the resource's value reaches the browser with
  the page, so hydration asks the bridge nothing.
- A mutation is a form post (§0); in the page, its server function. After
  a reload or a save the page counts a change (`load::changed`), and every
  read keyed on `load::changes` asks again, the shell's included.
- Only the visible tab reads: a tab's resources are created only on it.

## 7. The server half

- **One module owns every bridge call: `crate::bridge`.** It builds each
  request with the operator's token and reads the answer into the
  workspace's typed bodies (`fhir-types`, `openehr-mapping-core`,
  `ferrobridge-operator-api`), never reading a document by field name. Never a
  second client, a shadow type for a body the workspace already models, or a
  `reqwest` call from a page or a post (`.claude/rules/codegen.md`,
  `.claude/rules/spec-adherence.md`).
- A post handler and a server function are thin: check the session, call
  `crate::bridge`, render or answer. The two share their work (`posts`,
  `pages::editor::answer`, `pages::playground::answer`), which lives in
  plain modules with ordinary unit tests.
- Errors are typed: the bridge's `StatusCode` and body are carried as data
  (`BridgeError::Refused`), and a non-2xx answer is never an empty value.
- **An expected outcome is drawn as a page.** A read or a post yields a
  `Loaded<T>`: the document, or a bridge refusal with its status
  (`Outcome::Refused`), or the console's own fault (`ViewError`: a bridge it
  cannot reach, a body it cannot read). The page draws each in the section
  whose data it is. A caller branches on the three, so they are distinct
  types (`.claude/rules/reliability.md`, errors are types at a boundary that
  branches).
- Shared state reaches a render and a server function through
  `leptos_routes_with_context` and `provide_context`, and the plain axum
  handlers through `Extension` (`server/26_extractors`).
- **The security headers are the server's**, on every answer: a
  Content-Security-Policy with a nonce minted per response and no
  `'unsafe-inline'` or `'unsafe-eval'` for scripts, `nosniff`, `DENY`
  framing, no referrer, and `no-store` on every document. A new inline
  script or style that needs the policy relaxed is the defect, not the
  policy. `tests/it/server.rs` holds every header in-process.

## 8. SSR and hydration correctness (`ssr/22` to `ssr/24`)

- The whole tree runs on both halves, so the server's HTML and the
  hydrated view must be identical. Never branch a view's **structure** on
  `cfg!(target_arch = "wasm32")`. What the server read of the request
  reaches the browser through `crate::visit` (a `SharedValue`), never a
  cookie or the request read on the server alone (FerroUI
  `docs/consoles.md`, step 4); the visit carries the preference cookies and
  never the session's.
- Browser-only calls (`window`, storage, timers) go inside an `Effect`;
  a form's `on:submit` runs in the browser alone.
- **The shell is drawn once** (FerroUI `docs/consoles.md`, step 10): it
  calls `widget::nav::follow_path()` first, so the navigation marks the
  page in view and the rail's and the theme choice's `back` name it as the
  router moves. The shell is drawn again only when the session changes,
  and its top bar only when the visit does, never for the path, so the
  search's words and the navigation's scroll survive a link. A route group
  draws the page its address names as the address changes
  (`crate::routes`).
- Valid HTML and a deterministic first render are FerroUI §4; hydration
  walks the DOM, so a breach of either is a hydration error here. A view
  handed to a library slot that wraps it already (the error page's body is
  `<p>{body}</p>`) is text, never a second `<p>`. The journeys catch a
  mismatch as a browser panic; run them against the debug bundle
  (`scripts/release/console-site.sh` without `--release`), which names the
  element, when one fails.
- **A route whose content comes from the bridge renders `SsrMode::Async`**
  (`ssr/23_ssr_modes`): the server waits for every resource and sends the
  whole page, so nothing arrives in a `<template>` for an inline script to
  swap in, and the page reads with no script.
- The crate's `[package.metadata.leptos]` sets
  `disable-erase-components = true`, so a dev build keeps its component
  types in the browser half as the server half does and hydrates.
- **The hydration mark** (FerroUI `docs/consoles.md`, step 9). `hydrate()`
  calls `widget::hydration::explain()` after the panic hook, so a debug build
  names a page changed before hydration ended, then FerroUI's
  `widget::hydration::hydrate(App)`, which marks
  `<html data-ferroui-hydrated>` once the page and its route group have
  hydrated and leaves every click and submit to the browser until then. A
  journey, and the screenshot capture, wait for the mark before they act
  (`journeys::playground::hydrated`). The console keeps no mark of its own.
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
  `get_untracked` at setup. A page of a route group may read its params at
  setup, because its group draws it again whenever the path or the query
  changes (`crate::routes`).
- A link to an axum route the router must not follow (the sign-in, a
  download) carries `rel="external"` or `download`, as the library's own
  links do; the router leaves both to the browser.

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
  suite fails on a route without both. `journeys::navigation` marks the
  window and holds a followed link, the search's `GET` form, a form post,
  and the browser's back and forward to the same document, and shows its
  mark is lost on a real load. `journeys::no_wasm` refuses the browser the
  bundle and holds every navigation entry and a form post to their pages
  with nothing hydrated (FerroUI §15.7).
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
| WebAssembly, brotli-compressed | 661504 bytes |

- **The gating number is the brotli-compressed WebAssembly the page loads
  first.** The book's `deployment/binary_size` chapter has a site serve its
  WebAssembly compressed, every current browser accepts brotli, and the
  compressed size is the download. The raw and gzip sizes are reported beside
  it, ungated, and so are the lazy route chunks `--split` writes, which a
  page loads the first time it draws a page of that group, or earlier while
  the browser is idle.
- **The budget is 646 KiB**, a quarter over the 528557 bytes the move to
  FerroUI 0.5.0 measured (#1256, 2026-10-26, cargo-leptos 0.3.11,
  wasm-bindgen 0.2.129, wasm-opt 133, Rust 1.99.1): whole-page hydration
  puts the renderer, the reactive graph, the router, the shell, the visit
  and the reads' serde in the entry, and every data segment stays there,
  since `--split` moves code alone. The five route groups measured 61656
  (Bridge), 57221 (Console), 85562 (Mappings), 97004 (Overview) and 150861
  (Workbench, with the code editor), and the 25 chunks they share 301372.
  With About alone converted the entry measured 467850; carrying fhir-types
  R4 to the browser for the `CapabilityStatement` measured 583098, its
  regex's Unicode tables in the entry, so the facade and the playground draw
  the operator contract's reduced `capability::Capability`. FerroUI's own
  gallery, hydrated whole, measures 483968. Before it, 448 KiB was the
  islands-mode budget. The move to FerroUI 0.3.6 (#1168, 2026-10-24,
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
  library's switch together, and the document writes it as `<html lang>`.
  The browser takes the same choice from the visit (`crate::visit`) and
  provides it as well; the console's catalog is embedded in both halves.
  A language chosen in Your preferences loads the page as a document, since
  every word of a page is drawn in its locale.
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
