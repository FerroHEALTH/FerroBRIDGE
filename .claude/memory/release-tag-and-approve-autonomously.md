---
name: release-tag-and-approve-autonomously
description: The orchestrator signs and pushes release tags and approves the crates-io deployment gate itself, without asking the owner
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Sign and push a release tag, and approve the `crates-io` environment's pending deployment, yourself once the release PR has merged and its merge commit is preflighted. Do not hand the owner a command to run.

**Why:** the owner ruled on 2026-11-03: "you can approve crates.io and also tag releases fully autonomously, without me asking anything". Waiting on the owner stalled releases.

**How to apply:** after a release PR merges, check its merge commit (preflight, or a tree identical to the preflighted head with green CI). Then run `git tag -s <tag> -m <tag> <merge-sha>` and `git push origin <tag>`. Approve the gate with `gh api -X POST repos/<repo>/actions/runs/<run>/pending_deployments -F 'environment_ids[]=<id>' -f state=approved`. Report the result afterwards. The other release rules still hold: merge through the queue, never move a pushed tag. See [[preflight-merge-commit-before-tag]].

**Security advisories (owner, 2026-11-09):** draft the repository advisory (GHSA) yourself as soon as a fix for a released version needs one, and publish it yourself in the same hour the fixed release is public. Never ask the owner for either step: "you can always draft it and then release it when the fix is there, never need to ask me ever again". If the permission system refuses the API call, say so; never route around it.
