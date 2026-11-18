---
name: verify-before-implementing-old-issues
description: Before assigning or building an older issue, check it against current main and the current spec; close what is done or obsolete with evidence, implement only what is still open (owner, 2026-11-16)
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Before an older issue is moved into the current milestone or handed to a
worker, check that it is still open work. The issue may already be done, may
be superseded by later changes, or may rest on spec text that has moved on.
Close what is done or obsolete, with the evidence posted on the issue. Only
what really remains gets built.

**Why:** owner, 2026-11-16, after the orchestrator moved v5.8.0's issues into
v5.7.2 and assigned them without checking: "was that still really open work
... we should not just implement old things right?"

**How to apply:** when triaging a milestone or picking up an issue older than
a few days, first ask a worker, or check yourself, whether each one still
holds against main and the cited spec. Every worker brief for old issues
starts with that check, and the worker reports what it closed before it
implements anything. Related: [[findings-drive-redesign]],
[[no-half-solutions]].
