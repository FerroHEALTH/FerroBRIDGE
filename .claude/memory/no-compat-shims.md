---
name: no-compat-shims
description: Refactors break APIs freely and leave nothing behind; no deprecated items, compat aliases, re-exports, wrappers or stubs, and they run bottom-up (crates first, then the app)
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

A refactor in a family repository breaks internal and crate APIs as far as it needs to, and leaves no trace of the old shape. That means no `#[deprecated]` item, no alias or re-export kept for old callers, no shim, wrapper, stub or `todo!()` placeholder, and no "legacy" module. The removed item goes, and every caller in the workspace moves in the same pull request. Only the wire contract (ITS-REST, the served OpenAPI, the HTTP tests) stays fixed.

**Why:** the owner, 2026-11-01, during the FerroEHR #4212 and #4663 clean-up: "it's very very important that we need to make breaking changes", "we should not have legacy or backwards compatibility things", "I do not want stubs as well". The published `openehr-*` crates are 0.0.x and are versioned at publish (#4564), so a break costs nothing downstream.

**How to apply:** every refactor brief carries the no-compat line. A FerroEHR brief also carries that repository's own build rules, which the orchestrator broke on 2026-11-01. `tests-run-in-pr-ci.md` allows no local test suites: run fmt, clippy `--all-targets`, `cargo check` and rustdoc only, and let PR CI run nextest. `worktrees-for-parallel-branches.md` allows one `./target`: a worktree builds against the main checkout's `target`, never a second one. Because tests run only in CI, a worker waits for its PR's CI to go green before starting the next step. The work runs bottom-up. The `openehr-*` crates and `tools/` go first, each pull request fixing its callers in the app. Only then does the app layer go, through the one-worker sequence ([[remove-worktrees-after-merge]], #4212's plan comment). A crate change and an app refactor in flight at the same time would collide on the app's call sites. Related: [[workers-on-opus-only]].
