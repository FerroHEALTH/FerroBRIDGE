---
name: no-workflows-for-workers
description: Spawn workers with the Agent tool only, never the Workflow tool, because a workflow agent cannot be messaged
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Start every worker with the `Agent` tool (`model: 'opus'`, `run_in_background: true`). Never fan workers out through the `Workflow` tool.

**Why:** the owner, 2026-11-01. The five product workers ran in one workflow, and FerroTERM #956 then hit a merge conflict. A workflow agent does not appear in `ListAgents` and takes no `SendMessage`, so the orchestrator could not tell the worker. The orchestrator had to resolve the conflict itself.

**How to apply:** for parallel work, send several `Agent` calls in one message, within the cap of ten ([[worker-cap-ten]]). Each one can then be messaged with a correction, a follow-up issue or a conflict to fix. Related: [[workers-on-opus-only]], [[worker-wait-loops]].
