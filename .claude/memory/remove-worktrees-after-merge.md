---
name: remove-worktrees-after-merge
description: Remove every agent worktree, its branches and its build folder as soon as its PR is merged, in every family repo; never leave one hanging
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Remove a worktree under `.claude/worktrees/` and its local branches (`<branch>`, `worktree-agent-*`) as soon as the PR carrying its work is merged, and run `git fetch --prune`. A worktree whose work was discarded or superseded goes too. Check `git worktree list` at every merge notice.

**Why:** the owner found leftover worktrees and asked (2026-10-15) that they "never hang there".

**How to apply:** on each "PR merged" message: `git pull --ff-only`, `git worktree remove .claude/worktrees/<agent>`, `git branch -D <branch> worktree-<agent>`, `git fetch --prune`. If `worktree remove` leaves a non-empty folder, check it holds nothing unmerged, then move it out of the repo into the session scratchpad (the `block_dangerous` hook refuses `rm -rf` inside the repo). Keep a worktree only while its PR is open or its worker is running.

**Every family repository, and the build folders too:** the rule holds in FerroBRIDGE, FerroEHR, FerroFED, FerroTERM, FerroUI and FerroSMART, for the orchestrator and for every worker brief. Before removing a worktree, check it has no uncommitted or unpushed work and that its PR is merged. A worktree the session still locks for a finished agent cannot be removed; run `cargo clean` in it so its `target/` goes, since that is where the space is. Run `cargo clean` in a main checkout's `target/` too once no build uses it. The owner found the disk at 1.1 TB used on 2026-10-19 and asked to "keep cleaning the cargo and things"; the cleanup that day freed about 600 GB.

**Where:** every checkout, the orchestrator's own included, goes under `.claude/worktrees/<name>` (`git worktree add .claude/worktrees/<name> -b <branch> origin/main`), never a sibling folder beside the project. Each worktree builds into its own `target/` ([[worktree-target-dir]]). The owner objected on 2026-10-17 to `FerroBRIDGE-memwip` next to the repo: "why is this not in our main project?"
