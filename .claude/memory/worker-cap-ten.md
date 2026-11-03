---
name: worker-cap-ten
description: From 2026-11-03 the owner allows up to 10 concurrent implementation workers, replacing the cap of 5 that CLAUDE.md records
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Up to 10 implementation workers may run at once across the FerroHEALTH repositories. This replaces the cap of 5 that CLAUDE.md's model-orchestration section records (owner cap since 2026-10-19).

**Why:** the owner, 2026-11-03: "you are allowed to have 10 workers okay!!". It came after asking why so few workers ran while every product had open work.

**How to apply:**
- Keep the slots filled with real work across all products, as [[focus-ferroehr-refactor]] now says.
- Still give each worker disjoint files, so no two edit the same files at once.
- Read-only analysis agents count toward the 10 when they run long.
