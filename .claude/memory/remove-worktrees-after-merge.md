---
name: remove-worktrees-after-merge
description: Remove every agent worktree and its local branches as soon as its PR is merged; never leave one hanging
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Remove a worktree under `.claude/worktrees/` and its local branches (`<branch>`, `worktree-agent-*`) as soon as the PR carrying its work is merged, and run `git fetch --prune`. A worktree whose work was discarded or superseded goes too. Check `git worktree list` at every merge notice.

**Why:** the owner found leftover worktrees and asked (2026-10-15) that they "never hang there".

**How to apply:** on each "PR merged" message: `git pull --ff-only`, `git worktree remove .claude/worktrees/<agent>`, `git branch -D <branch> worktree-<agent>`, `git fetch --prune`. If `worktree remove` leaves a non-empty folder, check it holds nothing unmerged, then move it out of the repo into the session scratchpad (the `block_dangerous` hook refuses `rm -rf` inside the repo). Keep a worktree only while its PR is open or its worker is running.
