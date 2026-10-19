---
name: worktree-target-dir
description: Each worktree builds into its own target directory; never point CARGO_TARGET_DIR at another checkout's
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

A worktree under `.claude/worktrees/` builds into its own `target/`, the
default. Never set `CARGO_TARGET_DIR` to the main checkout's or another
worktree's target directory to save a build.

**Why:** two checkouts at different commits write into one directory, and
cargo then reads the other checkout's output. On 2026-10-19 the v0.0.9
release gate failed twice this way:

- A test build failed with E0308, "multiple different versions of crate
  `ferrohealth_ui`" (0.1.3 against 0.1.4), because one checkout's lockfile
  was ahead of the other's (#915).
- `run::serve_under_pretty_prints_the_banner_with_the_version_and_the_pins`
  failed at its version assertion. The `ferrobridge` binary under test was
  the main checkout's v0.0.8 build, while the test crate carried 0.0.9. The
  "Address already in use" line in its output is the bind failure the test
  provokes on purpose, not a port race (#914, closed as not planned).

The same rerun in the worktree's own target directory passed all 3407 tests.

**How to apply:** run `scripts/checks/fast.sh` and cargo in a worktree with
no `CARGO_TARGET_DIR` override. The first build in a fresh worktree is cold
and takes its time; run it in the background. A failure that names two
versions of one crate, or a binary reporting an older version than the
workspace, points at a shared target directory first.
