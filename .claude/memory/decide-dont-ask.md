---
name: decide-dont-ask
description: The orchestrator decides tracker, milestone, design and release questions itself and acts; it does not ask the owner to choose or run commands it can run
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Decide and act. Do not put a choice to the owner, and do not hand the owner a command to run when the orchestrator can run it itself. That covers which option to take, whether an issue is obsolete, reopening or closing an issue, moving a milestone, design rulings, and budget or ceiling calls on a pull request. For a performance trade-off, pick the option that leaves fewer allocations, less data through the system and more speed (owner 2026-11-23: "please do not ask me ... do the best what you think so we can get speed and less allocations and less data through the system").

**Why:** on 2026-11-03 the owner said "why are you asking me stupid things, you are in control, so you decide and just do things", after being asked about audit-format options and handed a command to reopen an issue.

**How to apply:** make the call, record the reasoning on the issue or PR, and report what was decided afterwards. Hand the owner a command only when the permission classifier has actually refused it to the orchestrator, and say that it was refused. Give a recommendation only when the owner asks for one. See [[release-tag-and-approve-autonomously]].
