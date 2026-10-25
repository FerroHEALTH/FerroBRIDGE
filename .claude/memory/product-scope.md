---
name: product-scope
description: "What FerroBRIDGE is (the owner's product statement), the standalone-server decision, the architecture of record from the issue #1 research, and that FHIR and OMOP are FerroBRIDGE's lane in the family, so FerroEHR carries no FHIR of its own"
metadata:
  node_type: memory
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The product statement, as the owner gave it, is the ceiling on what this
repository may claim:

> FerroBRIDGE is a pure-Rust, standalone bridge between openEHR and two
> interoperability targets: HL7 FHIR, driven by the FHIRconnect specification
> (model-mapping and contextual-mapping YAML validated against its published
> JSON schemas), and the OMOP Common Data Model, driven by the OMOCL
> specification. Mappings are specification-conformant YAML, never hand-coded
> per resource or table. It runs as its own server beside any openEHR CDR
> reached over the openEHR ITS-REST API (FerroEHR is the reference CDR, never a
> compile-time dependency) and uses a FHIR terminology server (FerroTERM is the
> reference) for code systems and value sets.

**Standalone server, decided.** FerroBRIDGE is its own process and its own
image. It is not a crate inside FerroEHR, not a plugin, and not a library a CDR
links. It reaches the CDR over ITS-REST like any other client, which is what
lets it work against a CDR it did not build.

**Mapping-driven, decided.** Mappings are specification-conformant YAML
validated against published schemas. A hand-coded per-resource or per-table
mapping defeats the premise, so it is refused in review
(`.claude/rules/rust-style.md`).

**Research first, on issue #1.** The crate layout, the engine design, the
generated model layers, the version pins and the acceptance instrument were
the research program's to settle, and `docs/architecture.md` is its output
and the design of record. Make no technical claim about a target beyond the
statement above, the architecture of record or a cited research finding
(`owner-work-style.md`).

**FHIR and OMOP are FerroBRIDGE's lane.** In the family architecture
([[family-architecture]]) FerroEHR implements no FHIR and no OMOP; both are
FerroBRIDGE's, and FerroBRIDGE in turn does no federation (FerroFED's) and no
patient identity or IHE profile (FerroPIX's). Any FHIR conversion still
inside FerroEHR is that repository's to remove, decided and executed there,
never from this repository (`sibling-projects.md`). Design here as if the CDR
has no FHIR surface of its own.

Update 2026-09-03 (research closed): `docs/architecture.md` exists; pins are FHIRconnect v1.0.0, FHIR R4, OMOCL v1.0.0, OMOP CDM v5.4, ITS-REST 1.1.0; one shared foundation, two interpreters, two sinks; FHIR facade + OMOP batch ETL; the FHIR model is a dependency, the CDM types are generated from the OHDSI definitions.

Update 2026-09-12 (third pass): `docs/architecture.md` is rewritten again. The
`fhir-types` move (#72) had not started; the sibling still publishes the crate
(0.1.97). The reference CDR retires its in-tree FHIR connector once the first
round trip ships (its #3080) and defers all profile mappings here (its #3206);
the carry-over register is architecture §12 and issue #1. The draft FHIRconnect
REST API (spec PR #93) is the engine surface; the EHDS categories and the HL7
Europe guides are the planned profile targets (§4.8, v0.0.8).
