---
name: pr-backlog-discipline
description: Keep the open pull requests few and tracked; never start new work while the merge backlog is high, never stack PRs on unmerged branches, and check the whole open list every turn
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-10-17 four workers, each told to open "one PR per issue", produced 21
open pull requests in a few hours. Several were stacked on branches that had
not merged; when a base merged squashed, its stack conflicted everywhere, one
squash silently landed three other PRs' changes (#691 carried #679, #683 and
#686), two PRs that both rewrote the server startup (#676, #690) collided,
and the merge took hours of rebuilding. The owner: "you went to fast and
didn't checked what you already had done and kept track of all the PR's ...
now we have an whole migration hell".

**Why:** throughput is the merged count, not the opened count. Every open PR
goes stale with each merge and every overlap costs a rebuild; a backlog grows
faster than a serial gate drains it.

**How to apply:**
- **Count before starting work.** At the start of every turn that could start
  or hand out work, run `gh pr list --state open` and read the number. With
  more than 6 open PRs, start no new worker and hand out no new issue: drain
  first (fix the red, resolve the conflicts, re-queue). Resume only at 3 or
  fewer.
- **One open PR per worker.** A worker brief names one issue, or a list
  worked strictly one at a time where the next issue starts only after the
  previous PR has MERGED, never "open each PR before starting the next".
  Parallel workers touch disjoint files; if two issues touch the same module
  (the server startup in `lib.rs`, `CHANGELOG.md` aside), they go to the same
  worker, in sequence.
- **No stacked PRs.** Never branch a PR from another open PR's branch. Wait
  for the base to merge, then branch from `main`. If a stack exists anyway,
  only the bottom PR is queued; each upper PR is rebuilt from `main` with
  only its own commits after its base merges (`pr-merge-watch.md`), and a
  squash that carried someone else's commits is checked (the change on
  `main`, the issue closed by hand with the merge commit named).
- **Track every open PR, not only the newest.** One watcher covers every open
  PR (`scripts/gh/watch-prs.sh $(gh pr list --state open --json number --jq
  '.[].number')`), restarted whenever a PR opens or the watcher stops. Each
  watcher stop is acted on at once.
- **Verify each PR as it opens:** `gh pr view <n> --json
  closingIssuesReferences` names exactly its issue (#645 named the wrong one),
  auto-merge or the merge queue holds it, and it is not a draft.
- **Report the open count** with every status message to the owner, so the
  backlog is visible.
- The merge queue (enabled 2026-10-17, `docs/ci-cd.md`) removes the
  up-to-date churn but not the overlap; this discipline is still needed.

Related: [[pr-merge-watch]], [[worker-wait-loops]], [[remove-worktrees-after-merge]].
