---
name: definitions-change-in-place
description: Until FerroEHR has a production release, shipped definitions (archetypes, templates) change in place at v1.0.0; never bump to v1.1.0 because an rc published them (owner, 2026-11-14)
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

A release candidate is not production. Until FerroEHR ships a real release (planned at 5.7.3), a shipped
definition is changed in place at its current version, and its digest is updated in
`deploy/definitions/published.sha256`. Never create a new version id (v1.1.0 and so on) just because
v5.7.1-rc1 published the definition. Fold any version created that way back into v1.0.0.

**Why:** the owner, 2026-11-14: "off course the v1.0.0 can chnage because nothng is in production". This
overrides the earlier rule that definitions released in rc1 get a new version.

**How to apply:** tell every worker that changes a definition to change it in place. Once a production
release exists, the never-change-under-its-id guard applies again. Related: [[new-design-no-legacy]].
