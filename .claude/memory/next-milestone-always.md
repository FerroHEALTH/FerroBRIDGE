---
name: next-milestone-always
description: Every FerroHEALTH repository keeps a next milestone open after the current one, so work that cannot finish in the active milestone has a home
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Each repository keeps at least one open milestone after the one being worked. Work that cannot be completed in the active milestone moves there, never into a milestone that has no due date, and never left without a milestone.

**Why:** the owner, 2026-11-03: "for the different repos do not forget to also create a new milestone after the current milestone where things can go in when they can't be completed during the active milestone you are working on".

**How to apply:**
- When a repository has only its active milestone, create the next one, `vX.Y.Z+1`, with a due date.
- When an issue cannot finish in time, move it to that milestone with a comment saying why, so the active milestone can still be cut.
- Done on 2026-11-03 for FerroFED (v0.0.17), FerroPIX (v0.0.14) and FerroUI (v0.6.5). FerroEHR, FerroBRIDGE and FerroTERM already had one.

Related: [[focus-ferroehr-refactor]].
