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

**How to apply:** every refactor brief carries the no-compat line. A FerroEHR brief also carries that repository's own build rules, which the orchestrator broke on 2026-11-01. `tests-run-in-pr-ci.md` allows no local test suites: run fmt, clippy `--all-targets`, `cargo check` and rustdoc only, and let PR CI run nextest. `worktrees-for-parallel-branches.md` puts each worktree at `.claude/worktrees/<name>` with its own target directory (owner delegation, 2026-11-09: a shared target lets one worktree overwrite another's workspace-member outputs). Because tests run only in CI, a worker waits for its PR's CI to go green before starting the next step. The work runs bottom-up. The `openehr-*` crates and `tools/` go first, each pull request fixing its callers in the app. Only then does the app layer go, through the one-worker sequence ([[remove-worktrees-after-merge]], #4212's plan comment). A crate change and an app refactor in flight at the same time would collide on the app's call sites. Related: [[workers-on-opus-only]].

**FerroBRIDGE is greenfield (owner, 2026-11-09):** nobody runs it yet, so a removed or renamed config key is simply deleted. There is no refusal that names the replacement and no upgrade fragment for it, because `deny_unknown_fields` already refuses the old key. Released products with deployments (FerroEHR) keep their named refusals for removed keys, since that is a refusal, not compatibility.

**One release, then deleted (owner, 2026-11-09, a hard rule for every FerroHEALTH product):** a removal aid lives for exactly one release. That covers a named refusal of a removed config key, a "renamed to" mapping, and any other trace of an old shape: an old CLI flag, a removed route's refusal, an old env variable, an old file format reader. The release that removes or renames the thing ships the refusal and the upgrade step in its changelog fragment. The next release deletes the refusal completely, and from then on `deny_unknown_fields` (or the plain absence of the route or flag) refuses the old form like any other unknown input. Nothing that serves an old shape is ever kept beyond that one release.

**Why:** the owner: "we keep this removed and mapped to only for one version and then in the next version you need to delete it fully so we keep always an clean codebase, otherwise some impl will only grow and grow with deleted or removed keys", and "this is not only for the config but also for other things!!! we should never support legacy".

**How to apply:** every brief that removes or renames something says the refusal is one release only. Each refusal records the version that introduced it (a `// NOTE:` or an entry in the refusal table naming the release), so the next release's cut can find it. A release PR, or the first PR after a release, deletes every refusal introduced in the previous release. A refusal older than one release that is found anywhere is deleted on sight, with a changelog entry under Removed.
