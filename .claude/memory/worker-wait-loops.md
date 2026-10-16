---
name: worker-wait-loops
description: Worker briefs forbid pgrep -f wait loops on their own command string; check for stuck shells when the owner reports many running
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Workers left shells polling for over an hour with `until ! pgrep -f "fhir-codegen -- emit"`: the pattern matches the loop's own command line, so it never ends. One loop from a finished, cleaned-up worker ran 4.5 hours. The owner noticed "13 shells running ... holding up things".

**Why:** each stuck loop holds a shell slot and burns CPU; the workers stack duplicate waits on the same output.

**How to apply:**
- Every worker brief says: run long commands with run_in_background and wait for the completion notification; never poll with `pgrep -f` on a string the loop itself contains; at most one wait per output.
- When the owner reports many shells, list them with `ps -eo pid,ppid,etime,command` plus `lsof -a -p <pid> -d cwd` to map each to its worktree, kill the self-matching and orphaned loops (worktree already removed), and message the owning worker.
- Related: [[pr-merge-watch]], [[subagent-reports-to-file]], [[remove-worktrees-after-merge]].

**Finished branches wait for nobody (owner, 2026-10-16):** workers held committed or nearly finished branches for one to three hours re-running the codegen drift check, the conformance gate and the whole workspace locally, while CI does all of it in about 14 minutes. The owner: "for 2 sub aganats we are still wiating for the last 2:40 hours!!! this is not acceptable". So a worker pushes once its own tests and `fast.sh` pass; when a worker's change is done but it is still waiting on local checks, the orchestrator stops it, commits, rebases and pushes the branch itself. Never stop processes with a broad `pkill -f <script>`: it kills every worker's run of that script; stop by PID or by the worker's task.
