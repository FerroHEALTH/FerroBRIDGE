---
paths: ["app/ferrobridge-console/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Operator console rules (`app/ferrobridge-console`)

**FerroUI leads the UI.** The console draws with `ferrohealth-ui`, pinned
exactly in the root `[workspace.dependencies]` (today `=0.6.0`), and the
FerroUI repository (FerroHEALTH/FerroUI) is the source for its components,
design, tokens, icons, script modules, accessibility bar, interaction
contract, words and quality control. Read these first, at the git tag that
matches the pin (today `v0.6.0`):

- [`docs/consoles.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.0/docs/consoles.md):
  the consumer guide, how a console uses the library.
- [`.claude/rules/console-ui.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.0/.claude/rules/console-ui.md):
  the component rules of every FerroHEALTH console. The section numbers this
  file cites (FerroUI §N) are those of the `v0.6.0` copy.
- [`crates/ferrohealth-ui/CLAUDE.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.0/crates/ferrohealth-ui/CLAUDE.md):
  the crate's own discipline and the interaction contract.
- [`design/consoles/ferrobridge/`](https://github.com/FerroHEALTH/FerroUI/tree/v0.6.0/design/consoles/ferrobridge):
  this console's pages, at 1440 and 390 pixels in both themes, which the
  screenshots are checked against.

The console moved to FerroUI 0.6.0 with #1355, the move FerroUI #510 decided
and `docs/migration-0.6.md` describes: no Leptos and no WebAssembly. Every
page is an axum handler's work that reads the bridge and returns the whole
document, drawn from the library's server-rendered components, and the
library's small script modules add behaviour on top of what the server drew.
FerroUI's `design/` is the only design source; a design folder anywhere else
is stale and is never synced from. A component this console needs and the
library lacks, or a component that behaves or looks wrong, is an issue filed
in FerroHEALTH/FerroUI and linked from the work here: never a local copy, a
shadow component, a stylesheet override or a hand-written script in this
repository.

This file keeps only what is particular to the operator console: its routes
and pages, the BFF and its session, the form posts, the bridge client and
the boundary, the configuration and security headers, the journeys and
screenshots, and the console's own words. The console's shape is section
4.10 of `docs/architecture.md`. No specification governs the console: our
own design.

## 0. The mandates (absolute)

- **Server-rendered HTML, no script of the console's own, no inline
  style**: FerroUI §0. The only JavaScript the console serves is the
  library's loader and modules, from the library's asset router; there is no
  npm, no Node, no CSS build and no WebAssembly.
- **The console is a client of the bridge and nothing else.** Its server
  reaches the bridge over HTTP, on the public surface any client uses,
  through `crate::bridge`, server to server. Of the workspace's own crates it
  links `openehr-mapping-core` (the `Diagnostic` a check answers),
  `fhir-types` with `r4` (the `OperationOutcome` and the
  `CapabilityStatement` it reads) and `ferrobridge-operator-api` (the
  bridge's health and operator documents, which the bridge serializes with
  the same types) and nothing else, and no Leptos crate or WebAssembly glue.
  `tests/it/boundary.rs` walks the resolved graph and fails on any other, so
  the console never compiles, transforms or saves a mapping itself: every
  check, transform and save goes through the bridge. A screen that needs
  what the bridge does not expose gets an endpoint of the bridge's operator
  API first, as its own issue, behind the bridge's token validation.
- **The console stores no clinical data, and holds no token in the
  browser.** The transform playground shows the operator what a `$tofhir` or
  `$toopenehr` answered, and nothing keeps it: no browser storage, no
  service worker, no cache (every page, the answer to a form post among
  them, is `Cache-Control: no-store`, held by a test), and no URL. The
  operator's session is server-side; the browser carries one opaque
  `HttpOnly` session cookie. An access token, a client secret, or a PKCE
  verifier never reaches the page HTML or a log.
- **No patient identifier in a URL, the browser history, or a log.** A
  patient is named through a request body, never a path segment or a query
  parameter, and a test over the rendered output and the request log proves
  it for each screen that takes one.
- **Every page and every action works with the scripts refused.** A link
  loads its page as one document. A preference is a post to a route of
  `crate::prefs`, which checks the session itself and answers `303 See
  Other` back to the page. Reload, the editor's check and save, and the
  playground's runs post to their own page's address (`crate::routes::posted`),
  whose route draws the page again with the bridge's answer and the posted
  fields and answers `200` (FerroUI `docs/consoles.md`, "An action that
  answers on its page"); the editor's Discard answers `303` to the file.
  Every page and every post checks the session before it asks the bridge,
  and the same-origin check refuses an unsafe method from anywhere else.
  `tests/it/forms.rs` lists every post and the pages that take none, holds
  each to a live session from the console's own pages, and holds the pages
  to carry no session cookie and no token. A new post joins those lists in
  the change that adds it.

## 1. Crate and build discipline

- One crate, one target, built by `cargo build`. A new dependency is a
  workspace-table entry, verified against crates.io the day it is added.
- **A page is a function of its module, `page(&Visit) -> Drawn`**, async
  when it reads the bridge, picked by path in `crate::routes::draw` behind
  the session's guard; `crate::routes::ROUTES` names every route, and the
  screenshot guard holds each to its images. A post a page takes is a
  function beside it, picked in `crate::routes::posted`.
- **Markup the console writes is escaped parts**: `crate::markup::h` over a
  `format!` whose every text is `crate::markup::t` (or a library part, which
  escaped its own text). A text the operator or the bridge supplied never
  reaches `h` unescaped.
- The workspace bar applies unchanged (`.claude/rules/reliability.md`,
  `.claude/rules/rust-style.md`): no `unsafe`, no `unwrap`/`expect` outside
  tests, `thiserror` for the console's error enums, every public item
  documented, suppressions as `#[expect(lint, reason = "…")]` scoped to the
  smallest item.

## 2. The request and its reads

- **One `crate::visit::Visit` per request**: the path and the query, the
  session and its operator, the preferences from their cookies (never the
  session cookie), the locale, the console's own settings, and the
  `crate::load::Reads` the page and the shell read the bridge through.
  `Reads` asks the bridge for a document once per request, so the shell's
  status pill and navigation count share what the page read, and nothing
  outlives the request.
- **An expected outcome is drawn as a page.** A read or a post yields a
  `Loaded<T>`: the document, or a bridge refusal with its status
  (`Outcome::Refused`), or the console's own fault (`ViewError`: a bridge it
  cannot reach, a body it cannot read). The page draws each in the section
  whose data it is; a refusal renders its status and the
  `OperationOutcome` or problem details it carried.
- **The page's state is in the HTML** (FerroUI `docs/consoles.md`, step 4):
  the tab is a path segment, the filter, search and paging live in the
  query, and the shell marks the page its path names. A value that can
  identify a patient never goes in the query.

## 3. Forms

The form rules are FerroUI §5: a stable `id` and `name` on every control
with its label's `for`, and no `GET` form for a field that can carry
clinical content. What is particular here:

- A form whose answer is the point (Reload, the editor, the playground) is
  a `<form method="post">` to its page's own address. Its route writes each
  posted field back into the form, escaped, beside the answer, and never a
  secret field.
- The decision of a form with several buttons is its submit button's
  `name="decision"` and `value` (the editor: `check`, `save`, `confirm`,
  `back`, `discard`); a decision that leaves the form is drawn with
  `formnovalidate` (`Decision::leaves`).
- A text area submits its line breaks as CRLF (HTML Living Standard
  §4.10.11); the editor's post reads the draft back with LF, so a save is
  the bytes the editor showed.
- Parse user input yourself. `type="number"` is not validation.

## 4. The server half

- **One module owns every bridge call: `crate::bridge`.** It builds each
  request with the operator's token and reads the answer into the
  workspace's typed bodies (`fhir-types`, `openehr-mapping-core`,
  `ferrobridge-operator-api`), never reading a document by field name.
  Never a second client, a shadow type for a body the workspace already
  models, or a `reqwest` call from a page or a post.
- A post handler is thin: check the session, call `crate::bridge`, draw.
  The work lives in plain modules (`posts`, `pages::editor::answer`,
  `pages::playground::answer`) with ordinary unit tests.
- **The security headers are the server's**, on every answer: the family's
  `ferrohealth_server::console` headers (`nosniff`, `DENY` framing, no
  referrer, a same-origin opener, `no-store` on every document) under the
  console's own Content-Security-Policy, `script-src 'self'; style-src
  'self'` with no nonce, no `'unsafe-inline'` and no `'unsafe-eval'`. A new
  inline script or style that needs the policy relaxed is the defect, not
  the policy. FerroUI's assets keep the year-long cache of their versioned
  path. `tests/it/server.rs` holds every header in-process.

## 5. Accessibility

The bar is FerroUI §7: **WCAG 2.2 Level AA** (<https://www.w3.org/TR/WCAG22/>)
in the light and the dark theme, keyboard operable with a visible focus, no
meaning on colour alone, reduced motion respected, reflow at 320 px with no
sideways scroll, the sticky top bar never covering the focus (SC 2.4.11). A
view uses the components and the role classes of `ferrohealth-ui`, never a
colour of its own. Tables carry real `<th scope=…>` headers, and a status is
a word as well as a tint.

## 6. Testing and gates

- **Business logic lives outside the markup**, in plain types with ordinary
  unit tests.
- The HTTP surface is tested in-process: `crate::server::router` driven with
  `tower::ServiceExt::oneshot`, and the bridge client against a `wiremock`
  stub bridge, with synthetic bodies.
- Gates for every console change: `cargo fmt` clean; `cargo clippy -p
  ferrobridge-console --all-targets -- -D warnings`; `cargo nextest run -p
  ferrobridge-console`.
- **Browser journeys** drive the console in headless Chrome through a
  WebDriver client in Rust, never Playwright. They live in
  `tools/ferrobridge-testkit/tests/it/journeys/` behind
  `FERROBRIDGE_JOURNEYS=1`, run against a running bridge and console in the
  `journeys (browser)` CI job, and fail on any `SEVERE` entry the browser
  logs. A journey that acts waits for the library's loader mark
  (`ferrohealth_ui::script::MARK`, `journeys::playground::loaded`).
  `journeys::navigation` holds a link, the search and a form post to one
  document each and a dialog to none; `journeys::no_script` refuses the
  browser FerroUI's modules and holds every navigation entry and a post of
  the playground and of the editor to their pages; `journeys::reflow` holds
  every page to 320 px with the modules loaded and refused. A new screen or
  route gets its journey and its light and dark screenshots
  (`FERROBRIDGE_SCREENSHOTS=1`, shown on the book's operator console page)
  in the same change; the screenshot guard of the ordinary suite fails on a
  route of `crate::routes::ROUTES` without both.
- **Never weaken a gate to make a change pass.**

## 7. Icons

The icon rule is FerroUI §9: `ferrohealth_ui::icon::Glyph`, the Lucide
outlines the library vendors, each beside the word it illustrates, and no
icon crate of the console's own. A glyph the console needs and the library
lacks is a FerroUI issue.

## 8. Strings and locales

The catalog rule is FerroUI §13: no string the console draws lives outside a
catalog, and a language joins as a catalog file with no code change. What is
particular here:

- The console's catalog is `app/ferrobridge-console/locales/<tag>/ferrobridge-console.ftl`,
  embedded by `fluent_templates::static_loader!` in `crate::locale`, where
  each message is one `Msg`. A test holds every catalog directory to the
  full message set, and a throwaway catalog under `tests/locales/` shows the
  words change with no code change.
- The server chooses the locale once per request (`crate::locale::requested`
  in `crate::visit::Visit::of`): the language of the operator's language
  cookie when the catalog holds it, else the best language of
  `Accept-Language` the catalog holds, else `[server] locale`. Every
  component takes that `&Locale`, so the console's words and the library's
  switch together, and the document writes it as `<html lang>`.
- Numbers go through the `format_number` formatter registered in the
  `static_loader!` customise hook, with the catalog's `number-group` and
  `number-decimal` messages.
- `scripts/checks/stray-literals.sh app/ferrobridge-console/src` runs in CI
  against `scripts/checks/stray-literals.baseline`, which names no file: a
  console string outside the catalog fails it. A literal no page draws (a
  protocol token, a header value, markup whose words are its placeables) is
  allowed in `scripts/checks/stray-literals.allow` with its reason, and the
  files that speak only to the terminal (`cli.rs`, `command.rs`, `config/`)
  are skipped there.
- A fault or a refusal the console draws reads its words from the catalog
  (`ViewError::text`, `Refusal::text`), and a plain-text answer of an axum
  route reads them in the locale `crate::locale::negotiated` picks from the
  request's headers. An error's `Display` is for the log alone, and never
  reaches a page.
