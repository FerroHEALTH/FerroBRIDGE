---
name: standalone-product
description: "FerroBRIDGE is presented as a standalone product: public documents never name FerroEHR or FerroTERM, the licence framing stands on its own, and the FHIR model crates are named by crate name (owner, 2026-09-04)"
metadata:
  node_type: memory
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

FerroBRIDGE is a standalone product. Its public documents (the README, the
changelog, `docs/`, the community and governance set, the committed scripts)
never name FerroEHR or FerroTERM. The bridge works with any openEHR CDR over
ITS-REST and any FHIR terminology server; the FHIR model comes from the
published `fhir-types` and `fhir-terminology` crates, named by crate name. The
licence section describes this project's own terms with no reference to
another project's.

**Why:** the owner, 2026-09-04, on the README licence paragraph that said "the
same terms as FerroEHR and FerroTERM": "you need to update the licence framing
because it's a standalone product", and then "I see you mention FerroTERM, do
not do that". Presenting the bridge as a satellite of two other products
misstates what it is and who it is for.

**How to apply:** before committing prose, `git grep -n -iE 'FerroTERM|FerroEHR'`
over the public tree must return nothing. The two projects remain read-only
prior art for the working discipline inside `.claude/` and `CLAUDE.md`
(`sibling-projects.md`); the memory files keep their history. Never cite a
sibling as the reason for a public statement; give the project's own reason.

Update 2026-10-08 (owner): every Ferro product works on its own and works
with the others only through the official specifications. FerroEHR (the
openEHR CDR), FerroTERM (the FHIR terminology server), FerroBRIDGE, FerroFED
(the federation tier) and FerroPIX (planned) form one family, and each must
serve anyone who uses one of them alone. So an interface between them is the
published one (FHIR R4, SMART App Launch, openEHR ITS-REST, the OMOP CDM,
OAuth 2.0), never a private shortcut. When a sibling needs something from
FerroBRIDGE (FerroEHR's chart, FerroFED's patient summary), build the
standard capability and describe it generically: "a Kubernetes deployment",
"a SMART client", never the sibling by name in public prose.
