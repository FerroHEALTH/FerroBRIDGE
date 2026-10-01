---
name: repo-moved-to-ferrohealth
description: 2026-10-01 the repo moved to FerroHEALTH/FerroBRIDGE; image ghcr.io/ferrohealth/ferrobridge (lowercase literal), board orgs/FerroHEALTH/projects/3; releases up to v0.0.4 stay signed as rubentalstra/FerroBRIDGE
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The repository was transferred from `rubentalstra/FerroBRIDGE` to the
`FerroHEALTH` organization on 2026-10-01 (#385), beside FerroEHR. The roadmap
board was copied to <https://github.com/orgs/FerroHEALTH/projects/3> with every
item and status (Projects cannot change owner); the old user board #6 is
retired.

- The image is `ghcr.io/ferrohealth/ferrobridge`, a literal in
  `release-image.yml`: `github.repository_owner` is `FerroHEALTH`, and an OCI
  reference must be lowercase.
- Still under the user account (do not rewrite): the SonarQube Cloud
  organization and key `rubentalstra_FerroBRIDGE`, the FerroEHR 4.2.5 and
  FerroTERM image pins under `ghcr.io/rubentalstra`, personal handles
  (CODEOWNERS, FUNDING, the roster), the released changelog sections.
- Releases up to v0.0.4 were signed as `rubentalstra/FerroBRIDGE` and their
  images live under `ghcr.io/rubentalstra/ferrobridge`; `SECURITY.md` says so.
- The transfer cleared the Pages custom domain, which was verified for the
  user account ([[domain-ferrobridge-eu]]); the org must verify it again.

**Why:** the owner moved the product line under one organization
([[sibling-projects]]).
**How to apply:** new links name `FerroHEALTH/FerroBRIDGE`; crates.io Trusted
Publishing entries name owner `FerroHEALTH`.
