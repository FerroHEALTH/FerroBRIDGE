---
name: merge-queue-light
description: The merge-group run is light (guards, rustfmt, cargo-deny, the crate-version guard, the reached packages' unit tests); the full suite runs on the pull request and on every push to main
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Owner's ruling (2026-11-25): the merge queue must be fast. The pull request
already ran the full suite, so the merge_group run does not run it again
(#2128). Queues of about ten entries dropped pull requests at the 60-minute
check timeout when every group re-ran the full tests, e2e, perf and
coverage. In merge_group CI runs tier 1, rustfmt, cargo-deny, the
crate-version guard and the `test` shards in smoke mode over the packages
the group reaches from its base: every test target compiled, only the unit
tests (`--lib`, or `--bins` for a binary-only package) run, since two pull
requests that passed alone can break a test binary together. Every other Rust lane carries
`github.event_name != 'merge_group'` in its job condition. `conclusion` keeps
its name and reads skipped as passed, so the ruleset needs no edit.

**How to apply:** a new lane that compiles, measures or starts a container
or a browser runs on the pull request and on main and is skipped in
merge_group at job level. A cheap guard that reads the merged tree (the
ratchets, the changelog and crate-version guards) runs in merge_group too.
Never change a threshold or a corpus to save time.
