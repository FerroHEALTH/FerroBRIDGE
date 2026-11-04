---
name: workers-own-every-open-pr
description: Every worker drives every PR it opened to merge, and the orchestrator sweeps all repos' open PRs so none sits unwatched
metadata:
  type: feedback
---

A worker owns every open PR it opened, not only its latest one. Before it starts new work, and while it waits, it lists its open PRs and drives each one to merge:
- arm auto-merge if it is off;
- fix a failing check;
- merge main in on a conflict;
- re-run a job that failed only on a GitHub outage.

The orchestrator also runs a shepherd in the combined watcher. It arms auto-merge on our own ready PRs that lack it, and closes and reopens once any Actions-bot PR with no CI, because a PR opened with GITHUB_TOKEN starts no workflows.

**Why:** on 2026-11-03 the owner saw many open PRs across the repos that no one was moving to merge. Workers watched only their newest PR, and hand-offs left older PRs unwatched.

**How to apply:** put this rule in every worker brief. When the owner asks about open PRs, list every repo's open PRs with their merge state, failing checks and queue entry before saying anything is fine. See [[workers-never-idle-on-merge-queue]].
