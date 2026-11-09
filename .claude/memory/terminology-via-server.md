---
name: terminology-via-server
description: Code translation in production always goes through the terminology server (FerroTERM is the reference); embedded or offline maps are for tests only
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-11-09 the owner overruled the plan to map FerroPIX's PID offline from
the v2-to-FHIR ConceptMaps embedded in ferrobridge-hl7v2: "why the fuck do we
not want a terminology server ... without terminology server makes absolutely
no sense". Offline means without a CDR, never without terminology.

**Why:** the family's design puts code systems and ConceptMaps in the
terminology server; a second copy inside a crate drifts from it.

**How to apply:** split by what a map is. Value maps (code A to code B, the
v2 tables) go only through ferrobridge-term against the terminology server,
never an embedded or offline copy. Structure maps (which field goes where, the
v2-to-FHIR message, segment and data type maps) are mapping logic: generated
at build time from the vendored, pinned guide, never loaded at runtime from a
folder or a server (owner agreed 2026-11-09). Tests use the testkit's
terminology stubs or a FerroTERM container. Never propose "no terminology
server" as a product posture.
Related: [[family-architecture]].
