---
name: merge-queue-functional-only
description: The merge queue runs only functional gates; perf and the browser journeys pass at once in merge_group and run on the PR and on every push to main
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Owner's family CI target (2026-11-09): pull-request and merge-group runs
finish in under 20 minutes each, queue time included (#1754). The merge
queue proves the merged tree, not the performance. `perf` and
`journeys (browser)` keep their job names. In merge_group, perf.yml gets
`measure: false` and its verdict passes with a summary note, and the journeys
job runs only a note step (#1790). On pull_request, on push to main and on a
release they run unchanged.

**How to apply:** a new long non-functional gate (fuzz, a full-corpus
sweep, a browser run) follows the same shape: it runs on the PR and on
main and passes with a note in merge_group. A functional gate (build,
clippy, tests, guards, codegen drift, conformance pass lists) always runs
in merge_group. Never change a threshold or a corpus to save time.
