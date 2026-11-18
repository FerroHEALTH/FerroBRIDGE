---
name: new-design-no-legacy
description: In a breaking release, build the new design in full and keep nothing of the old one; never relay a worker's "kept for compatibility" choice as a question (owner, 2026-11-14)
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

When a release is the breaking one (FerroEHR v5.7.2, the ABAC and access-log
redesign), the new design is built completely and every piece of the old one
is removed in the same program: no field kept "so old records read back", no
mark kept for one consumer, no claim-based gate kept beside the evaluator, no
default carried over from a removed key.

**Why:** the owner asked twice on 2026-11-14 why old parts were being kept
while the whole point of the breaking change was to do it the proper new way.
The orchestrator had been passing workers' compatibility choices on as
"decisions to confirm" instead of ruling them out.

**How to apply:** when a worker reports something kept for compatibility,
decide it yourself: file and fix its removal (FerroEHR #5586 and #5587 are
the pattern). Put the "no legacy" rule in every worker brief. Ask the owner
only about real design trade-offs, never "keep the old thing or not". Related:
[[no-half-solutions]], [[no-compat-shims]].
