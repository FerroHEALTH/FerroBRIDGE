---
name: worktrees-in-repo
description: Every git worktree in a family repo lives at <repo>/.claude/worktrees/<name>, never as a sibling folder next to the repo
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Every git worktree, in every FerroHEALTH repository, lives inside that repository at `<repo>/.claude/worktrees/<name>`, which `.gitignore` excludes. A worktree never goes in a sibling folder such as `RustroverProjects/FerroTERM-wt-778`.

**Why:** on 2026-10-21 the owner found worker worktrees scattered as sibling folders beside the repositories and called it custom logic they did not want.

**How to apply:** create worktrees with `git worktree add .claude/worktrees/<name>` from the repository root, and name that path in every worker brief. Move a stray one with `git worktree move`. Remove it, with its branch and `target/`, after the merge ([[worker-scratch-subfolders]]).

On 2026-10-27 the owner found it again: `FerroPIX-ui` and `FerroUI-states` sat beside the repositories as worktrees, and worker briefs had been placing worktrees under the session scratchpad. The scratchpad holds notes, PR bodies and logs only; a worktree is never a sibling folder and never in the scratchpad. Every worker brief names `<repo>/.claude/worktrees/<name>`, and a sweep for stray worktrees (`git worktree list` in each repo) belongs in the cleanup after each merge.
