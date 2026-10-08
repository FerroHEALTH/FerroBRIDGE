---
name: pr-licence-box
description: "Tick the pull request template's contribution-licence checkbox in the body when opening a pull request; the owner gave standing consent on 2026-10-08"
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Every pull request body ticks "I accept the terms in CONTRIBUTING.md §
Licensing of contributions" before `gh pr create`; the owner never ticks it
by hand.

**Why:** the owner, 2026-10-08: "i never need to check the license box,
please already do that for me when you create the PR". It is the owner's
standing acceptance as the contributor, and `contribution-licence-guard`
reads the box, so an unticked box stalls the merge.

**How to apply:** build the body from `.github/pull_request_template.md` with
the licence box and every verified checklist box ticked, open the pull
request, then arm auto-merge (`pr-auto-merge.md`).
