<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

---
name: ferropulse-private-no-public-logs
description: FerroPULSE stays private; its releases write nothing to public transparency logs, so its images carry SLSA L1 provenance, not L3
metadata:
  type: project
---

FerroPULSE (FerroHEALTH/FerroPULSE) stays a private repository, and its
release lane writes nothing to a public log: no cosign keyless signing, no
public Rekor entry. Its images carry BuildKit's in-index provenance and SBOM
(SLSA Build L1). Owner decision, 2026-11-10: "leave the L3, because the repo
will not be public".

**Why:** GitHub artifact attestations for private repositories need
Enterprise Cloud (the org is on Team), and the public Sigstore route would
publish the private repo's name, workflow, commits and digests.

**How to apply:** never wire attest-build-provenance, attest-sbom or public
cosign into FerroPULSE; its release notes and docs/release.md state the L1
level and why. The public products keep their L3 attestations.
