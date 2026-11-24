---
name: ferroehr-v6-major
description: The FerroEHR release now worked on as v5.7.2 will be renamed v6.0.0, a major release where everything is done the clean, proper, new way; older minor milestones (v5.8.0, v5.9.0) are emptied into it after a relevance check (owner, 2026-11-16)
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The owner plans to rename the current FerroEHR milestone v5.7.2 to v6.0.0,
because it carries major, breaking changes: the ABAC and access-log redesign,
the durable intake, the server 0.10 bound and more. The major is the moment to
do everything clean and proper, with a proper design throughout and no
legacy. The minor milestones v5.8.0 (closed 2026-11-16) and v5.9.0 are being
emptied into it. Each issue is checked for relevance first; see
[[verify-before-implementing-old-issues]].

**Why:** owner, 2026-11-16: "i want later to move 5.7.2 to v6 because there
is some major chnages ... this is the moment to do everything clean and
propper ... and also have an propper clean design for everything".

**How to apply:** treat the current cut as a major. Prefer the clean breaking
design over compatibility, and fold the cleanups that belong to a major into
it. The rename itself happens when the owner says so. This supersedes the
earlier "no release until 5.7.3" naming; the gate is still that everything is
properly implemented. Related: [[new-design-no-legacy]],
[[best-code-over-semver-bumps]].
