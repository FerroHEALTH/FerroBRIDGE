<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

---
name: publish-dispatch-approved
description: the orchestrator dispatches publish-crates and publish-chart itself whenever a release is ready; the owner approved it permanently
metadata:
  type: feedback
---

The orchestrating session dispatches every repository's `publish-crates.yml`
and `publish-chart.yml` itself (and approves their crates.io or registry
gate) as soon as the merged commit is ready, without asking. Owner,
2026-11-10: "you can run it yourself always when you think it's ready, fully
approved from my side always".

**Why:** two releases waited on the owner to run a one-line dispatch.

**How to apply:** only from `main` at a merged, green commit whose
preflight passes; confirm the version on crates.io or the chart's
attestation afterwards; workers still never dispatch a publish
([[release-tag-and-approve-autonomously]]).
