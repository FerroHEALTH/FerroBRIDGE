<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

---
name: orchestrator-files-cross-repo-gaps
description: the orchestrator files every cross-repo gap a worker finds, without asking the owner each time
metadata:
  type: feedback
---

The orchestrating session files every gap a worker finds in another family
repository itself, with type, priority, effort and the current milestone,
starts a worker on it, and tells the owner in the next summary. Owner
decision, 2026-11-10 ("File, fix; always file").

**Why:** workers are refused cross-repo filing by the permission classifier,
and asking the owner each time stalled releases (FerroTERM, FerroFED,
FerroEHR all waited on shared-crate gaps).

**How to apply:** a worker drafts the issue body in its scratch folder and
reports it; the orchestrator reads it, files it with `scripts/gh/fields.sh
new` in the owning repository, and routes the fix. The fix lands in the
owning crate or chart, never as a consumer-side adapter
([[no-half-solutions]], [[shared-crate-blockers]]).
