<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

---
name: watcher-always-on
description: the orchestrator's PR watcher must always run; after any shell cleanup it is the first thing restarted, and it flags unarmed PRs too
metadata:
  type: feedback
---

The orchestrating session keeps one PR watcher running across every product
repository at all times. After any shell cleanup or restart, the watcher is
the first thing brought back. It reports a red main, a conflicting PR, a
failing check, a milestone ready to cut, and an own PR that is ready but has
auto-merge off (dropped from the merge queue or never armed).

**Why:** on 2026-11-10 the watcher died in a shell cleanup and was not
restarted; for about two hours a conflict (FerroSYS #364) and a merge-queue
drop (FerroBRIDGE #1855) went unseen until the owner found them ("why was
nothing watching?").

**How to apply:** after any cleanup, check `ps` for the watcher before
anything else; never rely on workers' own monitors, since their runs end.
