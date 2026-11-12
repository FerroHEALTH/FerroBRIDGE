<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

---
name: focus-on-the-current-cut
description: work the current release milestone (and its patch line) before any later one; never merge later-milestone code before a pending cut
metadata:
  type: feedback
---

While a release is pending, every worker in that repository works its
milestone, then the next patch milestone, and nothing later. A worker
idle because the release waits on another repository takes the patch
milestone's small items into the current release, never a later minor.
Owner correction, 2026-11-10: "why is it making progress on 5.8.0? …
why we are not focusing on the things at hand?"

**Why:** the family's products release from main, so anything merged
before the tag ships in that release; later-milestone code would ride into
it unreviewed for that release.

**How to apply:** before giving a worker a later milestone, check that the
current one and its patch line hold nothing it can do; a later-milestone PR
opened early stays unarmed until the current tag is pushed.

2026-11-11 owner: "please stop working on things for 5.8.0 for FerroEHR, that is later". No worker starts v5.8.0 work, not even held drafts, until the owner says so. Existing drafts (#5369, #5378, #5401 to #5404) stay parked.
