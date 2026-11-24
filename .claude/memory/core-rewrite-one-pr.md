---
name: core-rewrite-one-pr
description: FerroEHR's hot-core rewrite (#6001) lands as one breaking PR; the branch may be broken while it is built, but it merges only green
metadata:
  type: project
---

The FerroEHR hot-core rewrite (#6001, v6.0.0: parse once at the edge, render once, no JSON inside the server, sub-issues #6002 to #6007 plus #5969 and #5997) is built as one complete, breaking PR, by owner decision on 2026-11-17. The branch may be broken while the rewrite is in progress. The PR merges only when its own CI is green and no test was weakened.

**Why:** the owner judged that a series of partial PRs would never merge, because each half-step breaks tests that only the whole change makes pass again.

**How to apply:** one long-lived branch from main for #6001, with main merged in regularly (signed merges, no rebase). The sub-issues are tracking checklists closed by that one PR (`Closes` per sub-issue). Workers coordinate on the one branch, not on separate PRs. The research issue #6006 is the exception: it is analysis and lands first. Related: [[quiet-host-perf-runs]].
