<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

---
name: cargo-semver-everywhere
description: every family repo that publishes a crate follows the Cargo SemVer chapter, checked by cargo-semver-checks; no home-made bump rules
metadata:
  type: feedback
---

Every FerroHEALTH repository that publishes a crate follows Cargo's SemVer
rules (the Cargo Book, "SemVer Compatibility"), checked mechanically by
cargo-semver-checks on pull requests and in the release plan, with one pinned
tool version across the family. Owner decision, 2026-11-10.

**Why:** the owner wants no invented versioning rules ("we should not invent
something ourselves"); a public API break that ships as a patch breaks every
consuming product, and the published standard plus its checker catch it.

**How to apply:** a repo's crates-publishing rule cites the Cargo SemVer
chapter as its authority; a breaking change in 0.x is a minor bump and a
public dependency's line move counts as breaking; a gap is filed in that
repo's current milestone and fixed there, never waived. Related:
[[no-compat-shims]].
