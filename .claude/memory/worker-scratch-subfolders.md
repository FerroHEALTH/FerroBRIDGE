---
name: worker-scratch-subfolders
description: Each worker writes scratch files under its own subfolder of the session scratchpad, never the root, and checks a file is its own before publishing it
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Every worker in a session shares one scratchpad directory. A worker writes
its scratch files (PR bodies, issue comments, notes, reports) under its own
subfolder of that scratchpad, named for its task (`<scratchpad>/<task>/`),
and never at the scratchpad root.

**Why:** on 2026-10-20 a FerroAUTH (then FerroSMART) worker left `pr1.md` (Closes #27 to #31
and #22) at the scratchpad root, and a FerroBRIDGE worker's
`gh pr edit 998 --body-file pr1.md` published it. #998 merged with that
body, so its own issue #997 did not auto-close and had to be closed by hand
(#1021). A generic file name at the shared root is a collision waiting for
the next worker.

**How to apply:**
- Every worker brief names the worker's own scratch subfolder and forbids
  the root.
- Before any `--body-file` or `-F` argument, the worker checks the file is
  one it wrote in this task: it lives under the worker's subfolder and its
  content names the worker's own issue.
- After setting a PR body, the worker re-reads it
  (`gh pr view <n> --json body,closingIssuesReferences`) and confirms the
  `Closes #N` lines name exactly its issues.
- Related: [[subagent-reports-to-file]], [[pr-licence-box]],
  [[worker-wait-loops]].
