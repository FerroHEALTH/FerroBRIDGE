---
name: workers-never-idle-on-merge-queue
description: A worker whose PR sits in a merge queue starts its next independent issue in a fresh worktree instead of waiting
metadata:
  type: feedback
---

A worker never idles while its PR waits in a merge queue. It leaves a background wait loop running, starts its next issue that touches none of the queued PR's files in a fresh worktree from origin/main, and comes back to the queued PR when the loop fires.

**Why:** on 2026-11-03 the owner saw workers running for 5 to 6 hours and asked if they had stalled. Each merge queue reruns the full CI (30 to 60 minutes per PR in FerroEHR and FerroBRIDGE), and workers were waiting on each merge before starting the next issue.

**How to apply:** put this in every worker brief. A queued PR shows `autoMergeRequest: null` with a `mergeQueueEntry`; read the entry through GraphQL before taking a null to mean auto-merge is off. When a worker has been idle for a while, check whether its PR is queued before deciding it is stuck. See [[pr-auto-merge]].
