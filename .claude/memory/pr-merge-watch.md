---
name: pr-merge-watch
description: "After opening a PR, keep watching it until it merges; auto-merge alone is not enough, and a red or conflicting PR is mine to fix right away"
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Arming auto-merge does not finish a PR. Every PR I open, and every open PR the owner hands me, is watched until it is merged, and whatever stops it is fixed by me without waiting to be asked.

**Why:** on 2026-10-15 the owner found PRs sitting red or blocked for an hour while I reported them as "armed": a `rustdoc` failure that came from `main` blocked four PRs at once, PRs merged before their own CI proved the fix, and changelog conflicts waited for the owner to notice. Their words: "you are not merging them always check it", "it should never get stuck".

**How to apply:**
- After `gh pr create` and `gh pr merge --auto --squash --delete-branch`, start a background watcher (run_in_background) on the PR: poll `gh pr view <n> --json state,mergeStateStatus,mergeable` and `gh pr checks <n>` every 60 s until `MERGED`. Act on each state:
  - `DIRTY` or `CONFLICTING`: merge `origin/main` into the branch, resolve (a `CHANGELOG.md` `[Unreleased]` conflict keeps both entries), re-run the affected gates, push.
  - `BEHIND`: `gh pr update-branch <n>`.
  - A failing required check (`conclusion`): read the failing job's log at once (`gh api repos/FerroHEALTH/FerroBRIDGE/actions/jobs/<id>/logs --allow-escape-sequences`, grep `^error`), fix it on the branch or, when the cause is on `main`, fix `main` first and then update every blocked PR.
  - Exit 143 with "runner has received a shutdown signal" is memory ([[ci-runner-memory]] in the repo), never a code defect.
  - `BLOCKED` with checks still pending: keep waiting; report what is still running and how long the slowest job (`codegen-emit-tests`, about 50 min) takes.
- When one red check shows on several PRs, the cause is on `main`: check `gh run list --workflow ci.yml --branch main` and fix it in the PR that merges first, bumping a crate only where the guard requires it.
- Never tell the owner a PR "will merge on its own" without a watcher running on it, and report each merge as it happens, then clean its worktree ([[remove-worktrees-after-merge]]).
- Advisory checks (SonarQube `analyze`, CodeQL) never block a merge; read them after the required gate is green.
- Every worker brief's gates include CI's rustdoc form: `RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --document-private-items -p <each touched crate>`. Without `--document-private-items` a public doc linking a private item passes locally and fails CI's `rustdoc` (it happened with #508 and #546).
- The watcher is `scripts/gh/watch-prs.sh <pr>...`, started with run_in_background from the repository root (zsh does not split an unquoted list, so never inline it). It updates a `BEHIND` branch, resolves a conflict confined to `CHANGELOG.md` (both `[Unreleased]` entries kept) and to version lines (the branch's higher version kept, `Cargo.lock` regenerated, the crate bump guard passing) and keeps watching, and exits 2 on any other conflict and 3 on a red required check, for a fix by hand.
