---
name: workers-on-opus-only
description: Every subagent and workflow agent runs on Opus; Fable is never used as a worker model
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Every worker is `model: 'opus'`, including research, hard planner work and reviews. Never pass `fable` to `Agent` or `Workflow`, whatever the CLAUDE.md orchestration table says about "Fable 5 subagents".

**Why:** the owner, 2026-10-30, after two FerroEHR AQL workers were started on Fable: "always use 5.5 opus is good enough" and "never ever use FABLE never allowed". Fable spends more of the usage budget than the work needs.

**How to apply:** set `model: 'opus'` on every spawn; a task that seems to need more intelligence gets a tighter brief and the handover of the previous worker, not a different model. Related: [[worker-wait-loops]], [[worker-scratch-subfolders]].
