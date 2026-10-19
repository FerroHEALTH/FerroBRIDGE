---
name: autonomous-loop
description: Never stop, never wait idle, never ask the owner; keep working the issue list autonomously while PRs drain
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The orchestrator never stops and never ends a turn just to wait. While the
watcher drains the open pull requests, workers run on the next issues that
touch other files. A question goes to the specifications, the code, the
tracker or the sibling projects, never to the owner.

**Why:** on 2026-10-17 the owner found the session idle behind a watcher with
five PRs open and wrote "you are never but never allowed to stop!!!" and "do
not ask my anything anymore if you need something!!! then look it up in the
specs ... you need to autonalsy keep going with all those 148 issues!!".

**How to apply:**

- A watcher or a worker in the background is never a reason to stop. Act on
  each notice at once, then start the next unit.
- Keep up to five workers busy (owner, 2026-10-19) on issues whose files do not overlap an open
  PR or another worker ([[pr-backlog-discipline]]). Same-module issues queue
  behind each other on one worker.
- A choice nobody has made gets the decision the specs support, recorded on
  the issue with its citation, or "no specification governs this: our own
  design". Never AskUserQuestion.
- A milestone at zero open issues is cut at once ([[release-tag-is-mine]]),
  and the loop moves to the next milestone.
- This replaces the pause rule in [[owner-work-style]] only when no pause was
  asked for.
