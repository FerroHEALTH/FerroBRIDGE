---
name: native-issue-types-and-priority
description: "Type, priority and effort are GitHub's native issue type and the FerroHEALTH Priority and Effort issue fields, set with scripts/gh/fields.sh; the bug, enhancement and P0 to P3 labels are retired; owner 2026-10-08, modelled on FerroFED (#416)"
metadata:
  node_type: memory
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Since 2026-10-08 FerroBRIDGE tracks the kind, urgency and size of its work
the way FerroFED has since 2026-10-02 (FerroHEALTH/FerroFED#154, merged in
FerroHEALTH/FerroFED#158): the native issue type (`Bug`, `Feature`, `Task`),
the organisation's `Priority` field (`Urgent`, `High`, `Medium`, `Low`) and
its `Effort` field (`High`, `Medium`, `Low`). The FerroHEALTH organisation
carries all three; its `Start date` and `Target date` issue fields stay
empty, because the milestone is the release spine.

**Why:** on 2026-10-08 the owner asked for the FerroFED model here: "please
high prio to also properly setup this in this repo". Labels for type and
priority duplicate what GitHub models natively, and the board and the issue
list can filter on the fields.

**How to apply:**

- File every issue with `scripts/gh/fields.sh new <type> <priority> <effort>
  <gh issue create args…>`; set or change one with `fields.sh type|priority|
  effort <n> <value>`; read with `fields.sh show <n>`.
- A Task carries exactly one work-kind label (`documentation`, `chore`,
  `refactor`, `perf`, `test`, `ci`); a Bug or a Feature carries none.
- Effort: `Low` is one sitting, `Medium` one pull request across crates or
  with a fixture, `High` more than one pull request or a held design. It
  never reorders the worklist; priority and age do.
- The migration is `scripts/gh/migrate-fields.sh plan|apply|verify`, and it
  runs before `scripts/gh/labels.sh` deletes the six old labels. Its judged
  tables cover the effort of every issue open on 2026-10-08, and the type,
  priority and work kind of the issues whose labels did not say.
- A scheduled lane's default token gets `organization: null` for the issue
  types; `fields.sh new` then files with the labels alone and whoever picks the
  issue up sets the three.
- Read an issue with `gh issue view <n> --json title,body,comments`; on gh
  2.102.0 `--comments` prints nothing for an issue without comments.
- The migration ran on 2026-10-08 (`scripts/gh/migrate-fields.sh apply`, 549
  changes over 227 issues, `verify` at `changes: 0`), and `labels.sh` retired
  `bug`, `enhancement` and `P0` to `P3` the same day.
- Related: [[repo-moved-to-ferrohealth]], [[pr-auto-merge]],
  [[upstream-reports-no-milestone]].
