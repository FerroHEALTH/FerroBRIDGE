---
name: merge-queue-signed
description: main merges through GitHub's merge queue (since 2026-10-17) with signed commits required; the strict up-to-date rule is gone, and gh pr update-branch --rebase still strips the signature
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-09-13 five green pull requests with auto-merge armed sat unmerged
for hours: the `main` ruleset required an up-to-date branch, so every merge
made the others stale, and required signed commits, so a branch rewritten by
`gh pr update-branch --rebase` (a server-side rebase that drops the author's
signature) was refused even with every check green. On 2026-10-17, with 21
open pull requests draining one per CI cycle, the owner approved the merge
queue: the ruleset now carries a `merge_queue` rule (squash, up to 5 entries
built and merged together, gated on `conclusion`) and the strict up-to-date
policy is off. `required_signatures` stays; the queue's merge commits are
GitHub's own.

**Why:** the queue tests queued pull requests together on top of `main`, so a
branch never has to be brought up to date by hand, and several land per CI
run.

**How to apply:** arm a pull request with `gh pr merge <n> --auto` (the
queue sets the method; passing `--squash` is refused). A conflicting branch
is still fixed by a LOCAL merge or rebase (commits signed by
`commit.gpgsign`) and pushed, never by `gh pr update-branch --rebase`.
Dependabot branches get the `@dependabot rebase` comment. Never `--admin`
without the owner asking. The queue does not prevent overlapping changes;
[[pr-backlog-discipline]] does.
