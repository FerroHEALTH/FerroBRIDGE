---
name: no-half-solutions
description: Build every issue the proper way it asks for, even when that needs a breaking change and a minor version bump; never shape a design to dodge semver
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Build what the issue asks for, the proper way. If the proper design adds a
field to a public struct or changes a signature, that is a breaking change:
bump the minor version (0.x to 0.(x+1)), rename the milestone, and write the
upgrade line in the changelog. Never tell a worker to "keep it additive" when
that forces a different, weaker design than the issue specifies, and never
accept a PR body that asks for an "owner decision" between the proper way
and a shortcut. Choose the proper way.

Before building, validate that what the issue asks for is what is needed.
Trace the requirement back to its consumer: who calls it, through what
interface, and to what end. Then confirm the requested shape is the right
one, and record the check on the issue. If the issue asks for the wrong
thing, correct the issue first; don't build it as written. Put the same
instruction in every worker brief.

**Why:** owner, 2026-11-08, on FerroAUTH #300; and the owner's follow-up the
same day: "you should always check and validate if what we should build is
also what we need to build". FerroAUTH #299 asked for a
`[<lane>.credentials.onward]` key to switch the onward token cache off. I had
told the worker to keep `ferrohealth-smart` 0.9.5 additive, so it built a
method on the client instead of the key. The owner called it a half solution:
"we do not want shortcuts or half solutions, we always do the proper way".

**How to apply:** when a worker brief names a release version, also say that
a breaking change moves the version, and that a version is never a reason to
narrow the design. Review each PR body for "owner decision needed" trade-offs
of this kind, and decide for the proper way. Related:
[[shared-crate-blockers]], [[ehr-crates-publish-approved]].
