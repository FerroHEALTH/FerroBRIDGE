---
name: sibling-projects
description: "The other FerroHEALTH products (FerroEHR, FerroFED, FerroPIX, FerroTERM) and the shared-crate repositories are read-only prior art from FerroBRIDGE, reached only over their published protocols; a sibling change is made in that sibling's checkout, and a tracker issue there may be filed when the owner asks"
metadata:
  node_type: memory
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

FerroBRIDGE is one of the five FerroHEALTH products ([[family-architecture]]
holds the table and the rules). The siblings it meets most are checked out
beside it on the owner's machine:

- **FerroEHR** at `../ferroehr`: the openEHR CDR. It is the **reference CDR**
  for FerroBRIDGE, reached over the openEHR ITS-REST API, and never a
  compile-time dependency; the bridge takes only its published `openehr-*`
  crates from crates.io. Its root `CLAUDE.md` and `.claude/rules/` are the
  deeper reference for this repository's own discipline, since FerroBRIDGE's
  configuration descends from it through FerroTERM.
- **FerroTERM** at `../FerroTERM`: the FHIR terminology server. It is the
  **reference terminology server** for code systems and value sets, reached
  over the FHIR terminology operations, and the project this repository's
  working configuration was modelled on.
- **FerroFED** (the federation gateway) and **FerroPIX** (the master patient
  index and the IHE profiles) are the other two products. FerroBRIDGE does
  no federation and no patient identity of its own; when it needs either,
  it calls that server over its protocol.
- The shared-crate repositories (FerroAUTH, FerroAUDIT, FerroUI) carry the
  cross-cutting work the bridge consumes as pinned crates.

**How to apply:**

- Read any sibling freely: its code, its rules, its git history, its closed
  issues. That is the fastest source of prior art for a decision here.
- **Exception: the FerroUI migration.** The owner signed off on 2026-10-18 on
  moving every console onto FerroUI, sibling repositories included
  ([[ferroui]]). It is standing: every FerroUI release made from here is
  rolled out to every family console from here too (owner, 2026-10-18).
- **Never edit a sibling from this repository on your own initiative.** A
  code change a sibling needs is made there, in its own session. Owner
  ruling 2026-09-12: when the owner directs a sibling change from a
  FerroBRIDGE session (the first was FerroTERM #300, consuming `fhir-types`
  from crates.io after the move), it is done in that sibling's checkout on
  its own branch and pull request, under that sibling's `CLAUDE.md`, rules
  and hooks, never by mixing it into a FerroBRIDGE branch.
- **A tracker issue in a sibling MAY be filed from here when the owner asks.**
  Owner ruling 2026-09-05 ("you just create an issue directly there because
  it's my repo"): a request the bridge needs from a sibling (the first was
  FerroTERM #300, widening `fhir-types` to the full resource set) is filed in
  that repository's tracker with its labels and milestone, and recorded here
  on the FerroBRIDGE issue that depends on it. Issues only; code stays theirs.
  A gap in a shared crate is filed and released in its owning repository,
  never copied here ([[family-architecture]]).
- No sibling is an oracle. A response from a running FerroEHR or FerroTERM is
  evidence in a comparison; the specification is the authority
  (`.claude/rules/spec-adherence.md`). A defect found in one of them is
  reported to that project rather than worked around here.
- The family is under the Business Source License 1.1 on the same terms
  since 2026-09-04 (see `license-busl.md`). Never copy code between the
  repositories anyway: each is its own Licensed Work with its own Licensor
  copy, and a licence decision is made per repository by the owner, never
  assumed from a sibling.
