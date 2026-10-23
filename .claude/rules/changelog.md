---
paths: ["CHANGELOG.md", "SECURITY.md", "docs/release.md", "docs/post-market.md", "scripts/release/**", "scripts/checks/changelog-*.sh", ".github/workflows/release.yml"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The changelog and the release cut

`CHANGELOG.md` follows Keep a Changelog 1.1.0, and a release's notes are its
changelog section, so what the section says is what users read. Every change
with user-visible effect adds an entry under `[Unreleased]` in the same pull
request (`CLAUDE.md` §Hard rules). The cut itself is `docs/release.md`.

## Structure

Each release section holds only the six Keep a Changelog types, `Added`,
`Changed`, `Deprecated`, `Removed`, `Fixed` and `Security`, each at most once:
a new entry merges into the existing subsection and never opens a second
header. `scripts/checks/changelog-structure.sh` checks the whole file in the
CI `versions` job and again in the release `plan` job. A released section is
also compared with its text at its tag (`scripts/checks/changelog-released.sh`),
so it is never edited after the release.

## Security fixes, advisories and the per-release reviews

Manufacturer duties under the Cyber Resilience Act, Regulation (EU)
2024/2847, Annex I Part II. Both rules are checked by
`scripts/checks/changelog-security.sh`: the CI `versions` job runs it over
`[Unreleased]` on every pull request, and the release `plan` job over the
section being cut. Released sections are not checked retroactively.
`scripts/release/preflight.sh vX.Y.Z` runs every check of the `plan` job
before the tag (`docs/release.md`).

- **A security fix ships alone, or says why not** (Annex I Part II(2): "where
  technically feasible, new security updates shall be provided separately
  from functionality updates"; `SECURITY.md` §Security advisories). A release
  whose `### Security` entries sit beside any other change subsection is
  refused unless its `### Security` section opens with a line
  `Shipped with functional changes because: <reason>`, naming why a
  security-only patch was not feasible. The release notes are extracted from
  that section, so the reason reaches them. "The next release was due anyway"
  is not a reason: cut a security-only patch from the last release's state
  instead.
- **Every `### Security` entry names its advisory** (Annex I Part II(4) and
  (8)). Each entry carries the `GHSA-…` identifier of the repository advisory
  published with the release, or, for a dependency bump that an OpenVEX
  document shows does not affect any shipped artefact, the path of that VEX
  statement under `security/vex/`, which must exist. An entry with neither
  refuses the cut. The advisory is drafted privately when the fix is found
  and published in the same hour as the GitHub release
  (`docs/post-market.md`, "A fixed vulnerability's advisory").
- **The previous release is taken by version, never by ancestry.**
  `bash scripts/release/plan.sh prev-tag vX.Y.Z` reads `git ls-remote` of
  origin and prints the first tag below the new one; a pushed tag counts
  whether or not its release published.
