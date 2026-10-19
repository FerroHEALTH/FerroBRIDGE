---
paths: ["app/ferrobridge-viewer/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Leptos viewer rules (`app/ferrobridge-viewer`, and any Leptos code)

The operator console's shape is section 4.10 of `docs/architecture.md`; this
file is the enforceable discipline. The oracle for every Leptos question is
the official Leptos book (<https://github.com/leptos-rs/book>, `main`, which
targets Leptos 0.8), never memory. Citations below are book chapters
(`view/04_iteration`, `ssr/23_ssr_modes`) and pinned crate documentation. The
discipline is the FerroHEALTH family's, carried from FerroFED's operator
console and FerroEHR's viewer, and adapted to this crate.

**The components, tokens, icons, accessibility bar and quality control are
FerroUI's**: the console draws with `ferrohealth-ui`, pinned exactly in the
root `[workspace.dependencies]`, and that library's own
`.claude/rules/leptos-ui.md` (FerroHEALTH/FerroUI) is the rule set for
every view. This file keeps what is particular to this console: the BFF, the
session, the server functions, the bridge client, the boundary and the
bundle budget. A component this console needs and the library lacks is added
to the library, never written here.

**The console is rendered on the server and hydrated in the browser**, with a
backend-for-frontend (BFF) on the same server, built by cargo-leptos. The two
halves are chosen by **compilation target, never by Cargo feature**: the host
build is the server half (`leptos/ssr`, axum, the bridge client), the
`wasm32-unknown-unknown` build is the browser half (`leptos/hydrate`). The
crate has no `ssr` or `hydrate` feature, so a workspace `--all-features`
build never turns both on. A change that brings back an `ssr`/`hydrate`
feature pair is an architecture change, returned to the orchestrator, never
made in a slice.

## 0. The mandates (absolute)

- **Rust only, zero hand-written JavaScript.** No authored `.js` files, no
  inline `<script>` bodies, no HTML `onxxx="..."` attributes carrying JS
  strings (the book's `oninput="this.form.requestSubmit()"` trick in
  `router/20_form` is forbidden here). Use an `on:` Rust listener.
  JS-wrapping crates are banned; a chart is drawn in Rust as SVG. The only
  JavaScript in the product is the `wasm-bindgen` glue and the hydration
  bootstrap the toolchain generates. There is no npm and no Node in the
  build, and no CSS build either: the server serves the library's
  stylesheet and the console's brand theme.
- **The console is a client of the bridge and nothing else.** Its server
  reaches the bridge over HTTP, on the public surface any client uses,
  through `crate::bridge`, server to server. Of the workspace's own crates
  the server half links `openehr-mapping-core` (the `Diagnostic` a check
  answers), `fhir-types` with `r4` (the `OperationOutcome` and the
  `CapabilityStatement` it renders) and `ferrobridge-operator-api` (the
  bridge's health and operator documents, which the bridge serializes with
  the same types) and nothing else; the browser half links
  none and receives view data from server functions. `tests/it/boundary.rs`
  walks the resolved graph and fails on any other workspace crate, so the
  console never compiles, transforms or saves a mapping itself: every check,
  transform and save goes through the bridge. A screen that needs what the
  bridge does not expose gets an endpoint of the bridge's operator API first,
  as its own issue, behind the bridge's token validation.
- **The console stores no clinical data, and holds no token in the
  browser.** The transform playground shows the operator what a `$tofhir` or
  `$toopenehr` answered, and nothing keeps it: no browser storage (local or
  session storage, IndexedDB), no service worker, no cache (every page and
  every server function answer is `Cache-Control: no-store`, held by a
  test), and no URL. The operator's session is server-side; the browser
  carries one opaque `HttpOnly` session cookie. An access token, a client
  secret, or a PKCE verifier never reaches a signal, a prop, a serialized
  resource, the page HTML, or a log.
- **No patient identifier in a URL, the browser history, or a log.** A
  patient is named through a request body or a form the BFF reads, never a
  path segment or a query parameter the router or the browser keeps, and a
  test over the rendered output and the request log proves it for each
  screen that takes one.
- **Every `#[server]` function is a public HTTP endpoint**
  (`server/25_server_functions`, the security warning). It checks the session
  itself before it touches the bridge or session state, never assuming only
  this UI calls it. **Every server function is a `POST`**: the server's
  same-origin check passes safe methods, and the session cookie travels on a
  cross-site top-level `GET`, so a `GetUrl` server function would run for
  any site. `tests/it/server_functions.rs` holds every registered function
  to both: its list must equal what `server_fn_paths()` registers, each
  entry is a `POST`, and each refuses a caller without a live session or
  from another origin before it asks the bridge anything. A new server
  function joins that list in the change that adds it.

## 1. Crate and build discipline

- One crate, two targets. Server-only dependencies live under
  `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`, browser-only
  ones under `[target.'cfg(target_arch = "wasm32")'.dependencies]`, and code
  for one half only carries the matching `#[cfg(target_arch …)]`. Keep the
  lib compiling for `wasm32-unknown-unknown` at all times: the CI `viewer`
  job runs clippy on that target.
- **Before the first `#[server]` function**, confirm against the Leptos book
  and a wasm32 clippy pass that the `#[server]` macro builds under the target
  split. If it needs a crate feature named `ssr`, stop and return the
  finding: the target split is section 4.10.
- **WebAssembly is 32-bit: never `usize`/`isize` in a server-function
  argument or return type, or in any serialized shared type.** Fixed-size
  integers only (`server/25` quirks).
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
  `scripts/release/viewer-site.sh --release`, and the bundle is held to the
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

- Component functions are **setup functions and run once**. Anything dynamic
  in a view is a signal or a closure reading signals
  (`reactivity/interlude_functions`).
- Access discipline (`reactivity/working_with_signals`): `.get()`/`.set()`
  for cheap `Copy`-ish values; `.read()`/`.write()` guards or
  `.with()`/`.update()` for collections. Never `sig.get().is_empty()`, which
  clones the whole value. Never hold a `.read()` guard across a write.
- A signal derived from a signal is a derived closure or a `Memo`. **Writing
  one signal from an `Effect` that reads another is forbidden**
  (`working_with_signals` §4). Effects only sync with the non-reactive
  outside world.
- Prefer local component state. Escalate in this order: the URL (router),
  then a context signal, then a `Store` (`15_global_state`). Context values
  use the newtype pattern; `expect_context` only where provision is
  structurally guaranteed.

## 3. Components and props

- Props that change over time are signal types (`view/03_components`): a
  reusable component takes `#[prop(into)] x: Signal<T>`.
- Doc-comment every component and every prop.
- Child to parent is a callback prop or an `on:` listener. Pass a
  `WriteSignal` down only where it is needed.

## 4. Views: iteration, control flow and errors

- Dynamic lists use `<For each key children>` with a **stable, unique,
  data-derived key, never an index** (`view/04_iteration`): a mapping file's
  path, a context name, a diagnostic's position and code.
- Expensive branches go behind `<Show when fallback>`; divergent branch types
  use `Either` or `.into_any()`.
- **An error never renders as nothing.** Resolve a `Result` where it arrives
  and render the content or an explicit inline error view in the section
  whose data failed. A bridge refusal renders its status and the
  `OperationOutcome` or diagnostics it carried.
- **Never create a resource inside a `Suspend` closure.** A `Suspend` re-runs
  on every notification of what it awaits, and each re-run re-creates what
  is inside it; resource ids then diverge between the server pass and
  hydration, and hydration reads the wrong serialized slot.
- **An anchor to a BFF axum route carries `rel="external"`** (the sign-in
  link): after hydration the client router intercepts same-origin anchors
  and 404s a route it does not own.

## 5. Forms

- Controlled inputs use `prop:value` with `on:input:target`, or the
  `bind:value`/`bind:checked`/`bind:group` sugar (`view/05_forms`). The
  `value` attribute only sets the initial value.
- A form that calls a server function dispatches its action from the
  browser's own form data: a plain `<form method="post">` whose `action` is
  the server function's URL, whose `on:submit` reads
  `web_sys::FormData::new_with_form`, calls `prevent_default` and dispatches
  the typed form. An `<ActionForm>` parses the URL-encoded form in the
  browser instead, about 26 KB of compressed WebAssembly in FerroFED's
  measurement, and is right only for a form that must work before the
  bundle loads (`progressive_enhancement/action_form`).
- Such a form has a plain-form fallback before hydration: its submit button
  is disabled until an `Effect` marks the page hydrated, and a plain post
  that still arrives is refused by the server function before it asks the
  bridge anything.
- **A `<textarea>` takes its text through `prop:value` alone, never a text
  child.** The server renders a textarea's child as written, `<` included,
  measured in #608, so a file holding `</textarea>` would end the element.
  The text reaches the browser in the serialized resource, JSON-escaped,
  and the field fills once the page hydrates, which its form waits for.
- Parse user input yourself. `type="number"` is not validation.
- Give every input an explicit, stable `id` and `name`, and associate the
  label with `for`.
- A form whose field can carry a patient identifier or clinical content (the
  playground's composition or Bundle) is never a `GET` form: its values would
  land in the URL and the history (§0).

## 6. Async data (the BFF pattern)

- Loading is `Resource::new(source, fetcher)` calling a server function; a
  mutation is a `ServerAction` (`async/10_resources`, `async/13_actions`).
  Reactive inputs go in the source; the fetcher is untracked.
- Read resources under `<Suspense>` on first load and under `<Transition>`
  when the resource refetches (`async/12_transition`).
- Never fetch inside an `Effect` and write a signal. `spawn_local` is for
  fire-and-forget only.
- Only the visible tab fetches.

## 7. The server half

- **One module owns every bridge call: `crate::bridge`.** It builds each
  request with the operator's token and reads the answer into the
  workspace's typed bodies (`fhir-types`, `openehr-mapping-core`,
  `ferrobridge-operator-api`), never reading a document by field name. Never a
  second client, a shadow type for a body the workspace already models, or a
  `reqwest` call from a server function body (`.claude/rules/codegen.md`,
  `.claude/rules/spec-adherence.md`).
- A server function is thin: check the session, call `crate::bridge`, map
  the error. Logic lives in plain modules with ordinary unit tests.
- Errors are typed: the bridge's `StatusCode` and body are carried as data
  (`BridgeError::Refused`), never stringified into
  `ServerFnError::ServerError`, and a non-2xx answer is never an empty value.
- **An expected outcome is a successful server-function answer.** A server
  function returns the view on success and a bridge refusal or input it
  cannot send as a typed refusal with the bridge's status; only the console's
  own faults are an error, and so a `5xx`: no live session, a bridge it
  cannot reach, a body it cannot read. A caller branches on the two, so they
  are distinct types (`.claude/rules/reliability.md`, errors are types at a
  boundary that branches).
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

- The app body runs on both halves, so the server HTML and the hydrated view
  must be identical. Never branch view **structure** on
  `cfg!(target_arch = "wasm32")`; a server-only side effect (setting the
  response status) is fine.
- Browser-only calls (`window`, storage, timers) go inside `Effect::new`;
  server-only code goes in a server function or the BFF modules.
- Views emit **valid HTML**: no block element inside `<p>`, and every
  `<table>` gets an explicit `<tbody>`. Hydration walks the DOM, so invalid
  HTML is a hydration error.
- No non-determinism in the initial render (random ids, timestamps) that
  differs between the server pass and hydration.
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

The bar is **WCAG 2.2 Level AA** (<https://www.w3.org/TR/WCAG22/>), checked
per slice. Every control is keyboard operable with a visible focus state;
tables carry real `<th scope=…>` headers; no meaning rides on colour alone,
so a status is a word as well as a tint; contrast meets AA in the light and
the dark theme, held by `tests/it/contrast.rs` over the stylesheet's role
tokens (4.5:1 for text, 3:1 for a control edge and the focus ring); motion
respects `prefers-reduced-motion`. A view uses the components and the role
classes of `ferrohealth-ui`, never a colour of its own, and the console's
brand theme is measured over the library's roles by the contrast test. The top bar stays in view from `md` up,
so the stylesheet's `scroll-padding-top` clears it for a focused control
(SC 2.4.11).

## 11. Testing and gates

- **Business logic lives outside components**, in plain types with ordinary
  unit tests. Components stay thin.
- The HTTP surface is tested in-process: `crate::server::router` driven with
  `tower::ServiceExt::oneshot` (`tests/it/server.rs`), and the bridge client
  against a `wiremock` stub bridge (`tests/it/bridge.rs`), with synthetic
  bodies.
- Gates for every console change: `cargo fmt` clean; `cargo clippy -p
  ferrobridge-viewer --all-targets -- -D warnings` on the host and `cargo
  clippy -p ferrobridge-viewer --lib --target wasm32-unknown-unknown -- -D
  warnings`; `cargo nextest run -p ferrobridge-viewer`;
  `scripts/release/viewer-site.sh --release` completing when the change
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
is held to a budget in CI. After `scripts/release/viewer-site.sh --release`,
the `viewer` job runs `scripts/checks/viewer-bundle.sh`, which writes the
WebAssembly and its JavaScript glue, raw, `gzip -9` and `brotli -q 11`, to
the job summary, and fails when the brotli-compressed WebAssembly is over the
budget below. The script reads the budget from this table, so the number
changes here and nowhere else.

| Measure | Budget |
|---|---|
| WebAssembly, brotli-compressed | 321536 bytes |

- **The gating number is the brotli-compressed WebAssembly.** The book's
  `deployment/binary_size` chapter has a site serve its WebAssembly
  compressed, every current browser accepts brotli, and the compressed size
  is the download. The raw and gzip sizes are reported beside it, ungated.
- **The budget is 314 KiB**, 25% over the 257500 bytes the mapping editor
  of #608 measured (2026-10-17, cargo-leptos 0.3.11, wasm-bindgen 0.2.129,
  Tailwind CSS 4.3.3, Rust 1.98.1). Its history:
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
  (`deployment/binary_size`). A document and a server function's answer are
  never compressed: each can carry what the operator entered beside data an
  attacker would want, which a compression side channel (BREACH) could read.
  `tests/it/server.rs` holds both.
- **The bundle names no directory of the build host.** The panic locations
  rustc embeds as data name the source file of every crate, so an
  unremapped CI build ships `/home/runner/...` and the registry and
  toolchain paths. `scripts/release/viewer-site.sh` remaps the cargo home,
  the toolchain sysroot and the checkout to `/cargo`, `/rustc-sysroot` and
  `/ferrobridge` for the WebAssembly build alone
  (`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS` with
  `--remap-path-prefix`; `trim-paths` is unstable), and the `viewer` job's
  `scripts/checks/viewer-paths.sh` fails when a home, runner, registry or
  toolchain path appears in the WebAssembly or its JavaScript glue. A
  `RUSTFLAGS` set in the environment overrides the target variable, and the
  check then fails.
- **The build tools are pinned and checked.** `scripts/release/viewer-tools.sh`
  installs Binaryen's `wasm-opt` at the release `docs/VERSIONS.md` pins,
  checked against its SHA-256, and `viewer-site.sh` refuses to run
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

- **Icons are `ferrohealth_ui::icon::Glyph`**, the Lucide outlines the
  library vendors; no icon crate of the console's own. A glyph the console
  needs and the library lacks is added to the library.
- An icon never carries meaning alone (§10): it sits beside the word it
  illustrates.
