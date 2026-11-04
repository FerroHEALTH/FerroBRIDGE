---
name: check-open-prs-proactively
description: The orchestrator checks every open PR for conflicts and failing checks itself, fixes main first when main is red, and works the issue list autonomously
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The orchestrator does not wait for a worker's report or for the owner to paste a red PR. After each merge and each worker hand-back, it lists every open PR in the repositories in play (`gh pr list` with `mergeStateStatus`, and `gh pr checks` for failing jobs) and fixes what it finds. A red `main` comes first, because every open PR inherits it. A background watcher keeps checking between turns and stops on the first failure or conflict. The issue list is then worked without being asked: when a slot is free, the next issue goes to a worker.

**Why:** the owner, 2026-11-01: "it's also very very important that you look for conflicts or issues please, because otherwise it will only grow and nothing will be merged" and "you need to do it autonomic okay, going through the issues". That day `main` had been red since #4669 on one stale test, and three open PRs also carried their own failures (a privacy checklist, a screenshot-guard label, a changelog fragment). Nobody noticed until the owner pasted them.

**How to apply:** use `gh pr update-branch` to bring PRs onto a fixed `main`. Fill a guard's review checklist (for example the privacy-boundary one) honestly from the diff, never as a formality. Related: [[pr-merge-watch]], [[pr-backlog-discipline]], [[no-workflows-for-workers]].
