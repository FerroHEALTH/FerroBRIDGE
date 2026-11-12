<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

---
name: best-code-over-semver-bumps
description: write the best code and take the breaking change; never shape an API around avoiding a version bump
metadata:
  type: feedback
---

When the best fix changes a public API, make that change and take the bump
Cargo SemVer requires (a minor in 0.x), with its cascade through the family.
Never keep a worse shape (a parallel variant, a leaked string, a duplicate
type) only to stay a patch. Owner rule, 2026-11-10: "breaking change is the
best way; we should not work around the best possible code".

**Why:** a workaround kept to dodge a bump stays in the API for good, and the
family carries no legacy ([[no-compat-shims]]); consumers move on the next
family round anyway.

**How to apply:** judge the change by [[cargo-semver-everywhere]], bump
honestly, update the dependent crates' requirements in the same round, and
tell each product worker the new line. A release in flight absorbs the bump
rather than shipping on the older line.
